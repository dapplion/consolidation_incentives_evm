use anyhow::{Context, Result};
use clap::Parser;
use proof_gen::{
    beacon_client::BeaconClient,
    types::preset::{SECONDS_PER_SLOT, SLOTS_PER_EPOCH},
    PendingConsolidationJson, ValidatorInfo,
};
use ssz_rs::HashTreeRoot;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Parser, Debug)]
#[command(name = "fetch-and-prove")]
#[command(about = "Fetch a real beacon-chain snapshot and summarize consolidation readiness")]
struct Args {
    /// Beacon API base URL
    #[arg(
        long,
        env = "GNOSIS_BEACON_URL",
        default_value = "https://rpc.gnosischain.com/beacon"
    )]
    beacon_url: String,

    /// State identifier to inspect (`finalized`, `head`, or a slot)
    #[arg(long, default_value = "finalized")]
    state_id: String,

    /// Gnosis genesis time used to derive beacon timestamps from slots
    #[arg(long, default_value_t = 1_638_993_340u64)]
    genesis_time: u64,

    /// Maximum number of pending consolidations to inspect in detail
    #[arg(long, default_value_t = 25)]
    max_consolidations: usize,

    /// Where to write the JSON snapshot
    #[arg(long, default_value = "real_chain_snapshot.json")]
    output: PathBuf,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
struct ConsolidationSnapshot {
    consolidation_index: usize,
    source_index: u64,
    target_index: u64,
    activation_epoch: u64,
    withdrawal_credentials: String,
}

#[derive(Debug, serde::Serialize)]
struct SnapshotMetadata {
    description: String,
    beacon_node: String,
    requested_state_id: String,
    resolved_state_id: String,
    state_source: String,
    finalized_epoch: u64,
    finalized_slot: u64,
    slot: u64,
    beacon_timestamp: u64,
    block_root: String,
    state_root: String,
    state_size_bytes: Option<usize>,
    debug_state_available: bool,
    total_pending_consolidations: usize,
    inspected_pending_consolidations: usize,
    credential_prefix_counts: BTreeMap<String, usize>,
    consolidations: Vec<ConsolidationSnapshot>,
    notes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = BeaconClient::new(args.beacon_url.clone());

    println!("🔍 Fetching real Gnosis beacon chain data...\n");
    println!("Using beacon endpoint: {}", args.beacon_url);
    println!("Requested state: {}\n", args.state_id);

    let finality = client
        .get_finality_checkpoints()
        .await
        .context("Failed to fetch finality checkpoints")?;

    let default_finalized_slot = finality.finalized_epoch * SLOTS_PER_EPOCH;
    let resolved_slot = resolve_slot(&args.state_id, default_finalized_slot, &client)
        .await
        .context("Failed to resolve requested state into a slot")?;
    let resolved_state_id = resolved_slot.to_string();

    println!("📍 Finality checkpoints:");
    println!("   Finalized epoch: {}", finality.finalized_epoch);
    println!("   Finalized slot:  {}", default_finalized_slot);
    println!(
        "   Finalized root:  0x{}\n",
        hex::encode(finality.finalized_root)
    );

    println!(
        "📦 Fetching beacon block header at slot {}...",
        resolved_slot
    );
    let header = client
        .get_header(&resolved_state_id)
        .await
        .context("Failed to fetch beacon block header")?;
    let block_root = header
        .hash_tree_root()
        .map_err(|e| anyhow::anyhow!("Failed to compute block root: {e:?}"))?;

    println!("   State root: 0x{}", hex::encode(header.state_root));
    println!("   Block root: 0x{}\n", hex::encode(block_root));

    println!("🧾 Fetching pending consolidations from standard Beacon API...");
    let pending_consolidations = client
        .get_pending_consolidations(&resolved_state_id)
        .await
        .context("Failed to fetch pending consolidations")?;
    println!(
        "   Found {} pending consolidations\n",
        pending_consolidations.len()
    );

    let mut consolidations = Vec::new();
    for (index, consolidation) in pending_consolidations
        .iter()
        .take(args.max_consolidations)
        .enumerate()
    {
        let validator = client
            .get_validator_info(&resolved_state_id, consolidation.source_index)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch validator {} for consolidation index {}",
                    consolidation.source_index, index
                )
            })?;
        consolidations.push(build_consolidation_snapshot(
            index,
            consolidation,
            validator,
        ));
    }
    let beacon_timestamp = args.genesis_time + (resolved_slot * SECONDS_PER_SLOT);

    println!("🧠 Inspecting validator metadata...");
    if consolidations.is_empty() {
        println!("   No pending consolidations in the inspected state");
    } else {
        for consolidation in &consolidations {
            println!(
                "   #{} source={} → target={} activation_epoch={} creds_prefix={}",
                consolidation.consolidation_index,
                consolidation.source_index,
                consolidation.target_index,
                consolidation.activation_epoch,
                credential_prefix(&consolidation.withdrawal_credentials)
            );
        }
    }
    println!();

    println!("🌲 Attempting debug-state SSZ fetch...");
    let (state_size_bytes, debug_state_available, mut notes) = match client
        .get_state_ssz(&resolved_state_id)
        .await
    {
        Ok(state_ssz) => {
            println!(
                "   State size: {} bytes ({:.2} MB)\n",
                state_ssz.len(),
                state_ssz.len() as f64 / 1_000_000.0
            );
            (
                Some(state_ssz.len()),
                true,
                vec![
                    "Debug state endpoint is available for this beacon node; full proof generation can proceed here.".to_string(),
                ],
            )
        }
        Err(error) => {
            println!("   Debug state fetch unavailable: {error}\n");
            (
                None,
                false,
                vec![
                    format!(
                        "Debug state endpoint unavailable for state {resolved_state_id}: {error}"
                    ),
                    "Standard Electra endpoints still confirmed pending consolidations + validator metadata.".to_string(),
                    "Full proof generation remains blocked until /eth/v2/debug/beacon/states/{state_id} is reachable.".to_string(),
                ],
            )
        }
    };

    if pending_consolidations.len() > args.max_consolidations {
        notes.push(format!(
            "Snapshot truncated to the first {} pending consolidations out of {} total.",
            args.max_consolidations,
            pending_consolidations.len()
        ));
    }

    let metadata = SnapshotMetadata {
        description: "Real beacon-chain consolidation snapshot".to_string(),
        beacon_node: args.beacon_url.clone(),
        requested_state_id: args.state_id,
        resolved_state_id,
        state_source: if debug_state_available {
            "standard endpoints + debug SSZ".to_string()
        } else {
            "standard endpoints only".to_string()
        },
        finalized_epoch: finality.finalized_epoch,
        finalized_slot: default_finalized_slot,
        slot: resolved_slot,
        beacon_timestamp,
        block_root: format!("0x{}", hex::encode(block_root)),
        state_root: format!("0x{}", hex::encode(header.state_root)),
        state_size_bytes,
        debug_state_available,
        total_pending_consolidations: pending_consolidations.len(),
        inspected_pending_consolidations: consolidations.len(),
        credential_prefix_counts: count_credential_prefixes(&consolidations),
        consolidations,
        notes,
    };

    fs::write(&args.output, serde_json::to_string_pretty(&metadata)?)
        .with_context(|| format!("failed to write {}", args.output.display()))?;

    println!("📊 Summary:");
    println!("   Slot: {}", metadata.slot);
    println!("   Beacon timestamp: {}", metadata.beacon_timestamp);
    println!(
        "   Pending consolidations: {}",
        metadata.total_pending_consolidations
    );
    println!(
        "   Debug state available: {}",
        metadata.debug_state_available
    );
    println!("   Snapshot saved to: {}", args.output.display());

    Ok(())
}

async fn resolve_slot(state_id: &str, finalized_slot: u64, client: &BeaconClient) -> Result<u64> {
    match state_id {
        "finalized" => Ok(finalized_slot),
        "head" => client.get_head_slot().await.map_err(anyhow::Error::from),
        other => other.parse::<u64>().with_context(|| {
            format!("unsupported state_id `{other}`; use finalized, head, or a slot")
        }),
    }
}

fn build_consolidation_snapshot(
    consolidation_index: usize,
    consolidation: &PendingConsolidationJson,
    validator: ValidatorInfo,
) -> ConsolidationSnapshot {
    ConsolidationSnapshot {
        consolidation_index,
        source_index: consolidation.source_index,
        target_index: consolidation.target_index,
        activation_epoch: validator.activation_epoch,
        withdrawal_credentials: format!("0x{}", hex::encode(validator.withdrawal_credentials)),
    }
}

fn credential_prefix(withdrawal_credentials: &str) -> &str {
    withdrawal_credentials
        .get(0..4)
        .unwrap_or(withdrawal_credentials)
}

fn count_credential_prefixes(consolidations: &[ConsolidationSnapshot]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for consolidation in consolidations {
        *counts
            .entry(credential_prefix(&consolidation.withdrawal_credentials).to_string())
            .or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_slot_for_literal_slot() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = BeaconClient::new("http://127.0.0.1:1");
        let slot = runtime
            .block_on(resolve_slot("12345", 999, &client))
            .unwrap();
        assert_eq!(slot, 12345);
    }

    #[test]
    fn resolve_slot_for_finalized() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = BeaconClient::new("http://127.0.0.1:1");
        let slot = runtime
            .block_on(resolve_slot("finalized", 777, &client))
            .unwrap();
        assert_eq!(slot, 777);
    }

    #[test]
    fn resolve_slot_rejects_invalid_state_id() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = BeaconClient::new("http://127.0.0.1:1");
        let error = runtime
            .block_on(resolve_slot("weird", 777, &client))
            .unwrap_err();
        assert!(error.to_string().contains("unsupported state_id"));
    }

    #[test]
    fn build_snapshot_preserves_validator_details() {
        let consolidation = PendingConsolidationJson {
            source_index: 42,
            target_index: 99,
        };
        let validator = ValidatorInfo {
            withdrawal_credentials: [0x02; 32],
            activation_epoch: 123,
        };

        let snapshot = build_consolidation_snapshot(7, &consolidation, validator);
        assert_eq!(snapshot.consolidation_index, 7);
        assert_eq!(snapshot.source_index, 42);
        assert_eq!(snapshot.target_index, 99);
        assert_eq!(snapshot.activation_epoch, 123);
        assert_eq!(
            snapshot.withdrawal_credentials,
            format!("0x{}", "02".repeat(32))
        );
    }

    #[test]
    fn count_credential_prefixes_groups_by_first_byte() {
        let consolidations = vec![
            ConsolidationSnapshot {
                consolidation_index: 0,
                source_index: 1,
                target_index: 2,
                activation_epoch: 3,
                withdrawal_credentials: format!("0x01{}", "00".repeat(30)),
            },
            ConsolidationSnapshot {
                consolidation_index: 1,
                source_index: 4,
                target_index: 5,
                activation_epoch: 6,
                withdrawal_credentials: format!("0x02{}", "11".repeat(30)),
            },
            ConsolidationSnapshot {
                consolidation_index: 2,
                source_index: 7,
                target_index: 8,
                activation_epoch: 9,
                withdrawal_credentials: format!("0x01{}", "22".repeat(30)),
            },
        ];

        let counts = count_credential_prefixes(&consolidations);
        assert_eq!(counts.get("0x01"), Some(&2));
        assert_eq!(counts.get("0x02"), Some(&1));
    }
}
