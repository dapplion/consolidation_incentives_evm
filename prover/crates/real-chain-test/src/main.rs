use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use proof_gen::{
    beacon_client::{BeaconClient, BeaconClientError},
    types::preset::{SECONDS_PER_SLOT, SLOTS_PER_EPOCH},
    FinalityCheckpoints, PendingConsolidationJson, ValidatorInfo,
};
use ssz_rs::HashTreeRoot;
use std::{collections::BTreeMap, fs, path::PathBuf};
use tokio::time::{sleep, Duration};

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

    /// When set, scan finalized slots in [scan_start_slot, scan_end_slot] and stop at the
    /// first state with pending consolidations. Defaults to the resolved state slot.
    #[arg(long)]
    scan_start_slot: Option<u64>,

    /// Inclusive end of the historical scan window. Required when --scan-start-slot is set.
    #[arg(long)]
    scan_end_slot: Option<u64>,

    /// Historical scan start epoch. Converted to the first slot in that epoch.
    /// Cannot be combined with --scan-start-slot or --scan-last-epochs.
    #[arg(long)]
    scan_start_epoch: Option<u64>,

    /// Historical scan end epoch. Converted to the last slot in that epoch.
    /// Cannot be combined with --scan-end-slot or --scan-last-epochs.
    #[arg(long)]
    scan_end_epoch: Option<u64>,

    /// Scan the last N finalized epochs ending at the finalized slot.
    /// Handy when you want a recent-history sweep without doing slot math by hand.
    #[arg(long)]
    scan_last_epochs: Option<u64>,

    /// Slot stride used during historical scans. Defaults to one finalized epoch on Gnosis.
    #[arg(long, default_value_t = SLOTS_PER_EPOCH)]
    scan_step_slots: u64,

    /// Historical scan direction. `reverse` is handy when you want the latest non-empty state first.
    #[arg(long, value_enum, default_value_t = ScanDirection::Forward)]
    scan_direction: ScanDirection,

    /// Stop a historical scan after collecting this many non-empty states.
    /// Useful when you only need the first/latest few hits instead of sweeping the entire window.
    #[arg(long)]
    scan_hit_limit: Option<usize>,

    /// Gnosis genesis time used to derive beacon timestamps from slots
    #[arg(long, default_value_t = 1_638_993_340u64)]
    genesis_time: u64,

    /// Watch finalized states until a non-empty pending_consolidations state appears.
    /// Useful when historical state retention is limited and you need to capture the state live.
    #[arg(long)]
    watch_finalized: bool,

    /// Poll interval in seconds for --watch-finalized.
    #[arg(long, default_value_t = SECONDS_PER_SLOT)]
    watch_poll_seconds: u64,

    /// Optional cap on finalized-state polls before exiting.
    #[arg(long)]
    watch_max_polls: Option<usize>,

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
    scan_window: Option<ScanWindow>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ScanDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct ScanHit {
    requested_slot: u64,
    slot: u64,
    epoch: u64,
    pending_consolidations: usize,
}

#[derive(Debug, serde::Serialize)]
struct ScanWindow {
    start_slot: u64,
    end_slot: u64,
    start_epoch: u64,
    end_epoch: u64,
    slots_checked: usize,
    scan_step_slots: u64,
    scan_direction: String,
    scan_hit_limit: Option<usize>,
    first_non_empty_slot: Option<u64>,
    last_non_empty_slot: Option<u64>,
    first_non_empty_epoch: Option<u64>,
    last_non_empty_epoch: Option<u64>,
    non_empty_slots: Vec<ScanHit>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = BeaconClient::new(args.beacon_url.clone());

    println!("🔍 Fetching real Gnosis beacon chain data...\n");
    println!("Using beacon endpoint: {}", args.beacon_url);
    println!("Requested state: {}\n", args.state_id);

    let initial_finality = client
        .get_finality_checkpoints()
        .await
        .context("Failed to fetch finality checkpoints")?;

    let default_finalized_slot = initial_finality.finalized_epoch * SLOTS_PER_EPOCH;
    let requested_slot = resolve_slot(&args.state_id, default_finalized_slot, &client)
        .await
        .context("Failed to resolve requested state into a slot")?;
    let (resolved_slot, pending_consolidations, scan_window, watch_summary, finality) =
        resolve_target_state(
            &args,
            &client,
            requested_slot,
            default_finalized_slot,
            &initial_finality,
        )
        .await?;
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
    println!(
        "🧾 Using {} pending consolidations from state {}\n",
        pending_consolidations.len(),
        resolved_state_id
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

    if let Some(scan_window) = &scan_window {
        match scan_window.first_non_empty_slot {
            Some(slot) => {
                notes.push(format!(
                    "Historical scan checked {} finalized slots and found the first non-empty pending_consolidations state at slot {slot}.",
                    scan_window.slots_checked
                ));
                if let Some(last_slot) = scan_window.last_non_empty_slot {
                    notes.push(format!(
                        "Historical scan observed {} non-empty states spanning slots {}..={}.",
                        scan_window.non_empty_slots.len(),
                        slot,
                        last_slot
                    ));
                }
            }
            None => notes.push(format!(
                "Historical scan checked {} finalized slots ({}..={}) and found no pending consolidations.",
                scan_window.slots_checked, scan_window.start_slot, scan_window.end_slot
            )),
        }
    }

    if let Some(watch_summary) = &watch_summary {
        if watch_summary.found_non_empty_state {
            notes.push(format!(
                "Finalized watch captured a non-empty state after {} poll(s); first hit slot {} (epoch {}).",
                watch_summary.polls,
                watch_summary.resolved_slot,
                watch_summary.resolved_slot / SLOTS_PER_EPOCH
            ));
        } else {
            notes.push(format!(
                "Finalized watch ended after {} poll(s) without finding pending consolidations; latest finalized slot checked was {}.",
                watch_summary.polls,
                watch_summary.resolved_slot
            ));
        }
    }

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
        scan_window,
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
    if let Some(scan_window) = &metadata.scan_window {
        println!(
            "   Scan window: slots {}..={} / epochs {}..={} ({} slots checked)",
            scan_window.start_slot,
            scan_window.end_slot,
            scan_window.start_epoch,
            scan_window.end_epoch,
            scan_window.slots_checked
        );
        if !scan_window.non_empty_slots.is_empty() {
            println!(
                "   Non-empty slots found: {} (first slot={} / epoch={}, last slot={} / epoch={})",
                scan_window.non_empty_slots.len(),
                scan_window.first_non_empty_slot.unwrap_or_default(),
                scan_window.first_non_empty_epoch.unwrap_or_default(),
                scan_window.last_non_empty_slot.unwrap_or_default(),
                scan_window.last_non_empty_epoch.unwrap_or_default()
            );
        }
    }
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

#[derive(Debug, Clone, serde::Serialize)]
struct WatchSummary {
    polls: usize,
    poll_interval_seconds: u64,
    max_polls: Option<usize>,
    resolved_slot: u64,
    found_non_empty_state: bool,
}

async fn resolve_target_state(
    args: &Args,
    client: &BeaconClient,
    requested_slot: u64,
    finalized_slot: u64,
    finality: &FinalityCheckpoints,
) -> Result<(
    u64,
    Vec<PendingConsolidationJson>,
    Option<ScanWindow>,
    Option<WatchSummary>,
    FinalityCheckpoints,
)> {
    if args.watch_finalized {
        anyhow::ensure!(
            !has_scan_window(args),
            "--watch-finalized cannot be combined with historical scan flags"
        );
        anyhow::ensure!(
            !has_non_default_scan_controls(args),
            "--watch-finalized cannot be combined with scan-step-slots, scan-direction, or scan-hit-limit"
        );
        anyhow::ensure!(
            args.state_id == "finalized",
            "--watch-finalized currently requires --state-id finalized"
        );

        let (resolved_slot, pending_consolidations, watch_summary, finality) =
            watch_finalized_state(args, client, finality).await?;
        return Ok((
            resolved_slot,
            pending_consolidations,
            None,
            Some(watch_summary),
            finality,
        ));
    }

    let Some((scan_start_slot, scan_end_slot)) = resolve_scan_window(args, finalized_slot)? else {
        let pending_consolidations = client
            .get_pending_consolidations(&requested_slot.to_string())
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch pending consolidations for state {}",
                    requested_slot
                )
            })?;
        return Ok((
            requested_slot,
            pending_consolidations,
            None,
            None,
            finality.clone(),
        ));
    };

    anyhow::ensure!(
        scan_start_slot <= scan_end_slot,
        "scan_start_slot must be <= scan_end_slot"
    );
    anyhow::ensure!(
        scan_end_slot <= finalized_slot,
        "scan_end_slot {scan_end_slot} exceeds finalized slot {finalized_slot}"
    );
    anyhow::ensure!(
        requested_slot >= scan_start_slot && requested_slot <= scan_end_slot,
        "resolved state slot {requested_slot} must be inside the scan window {scan_start_slot}..={scan_end_slot}"
    );

    anyhow::ensure!(
        args.scan_step_slots > 0,
        "scan_step_slots must be greater than zero"
    );

    let scan_slots = build_scan_slots(
        scan_start_slot,
        scan_end_slot,
        args.scan_step_slots,
        args.scan_direction,
        requested_slot,
    );

    println!(
        "🕰️  Scanning finalized slots {}..={} for pending consolidations (step={}, direction={})...",
        scan_start_slot,
        scan_end_slot,
        args.scan_step_slots,
        args.scan_direction.as_str()
    );

    let mut slots_checked = 0usize;
    let mut fallback_pending = None;
    let mut first_hit: Option<(u64, Vec<PendingConsolidationJson>)> = None;
    let mut non_empty_slots = Vec::new();

    validate_scan_hit_limit(args.scan_hit_limit)?;

    for requested_scan_slot in scan_slots {
        let (resolved_scan_slot, pending_consolidations) =
            fetch_pending_consolidations_at_or_before(client, requested_scan_slot, scan_start_slot)
                .await
                .with_context(|| {
                    format!(
                "Failed to fetch pending consolidations for slot {} or earlier within scan window",
                requested_scan_slot
            )
                })?;
        slots_checked += 1;

        if requested_scan_slot == requested_slot {
            fallback_pending = Some(pending_consolidations.clone());
        }

        if resolved_scan_slot != requested_scan_slot {
            println!(
                "   Requested slot {} had no state; fell back to slot {}",
                requested_scan_slot, resolved_scan_slot
            );
        }

        if !pending_consolidations.is_empty() {
            println!(
                "   Found {} pending consolidations at slot {} (requested {})",
                pending_consolidations.len(),
                resolved_scan_slot,
                requested_scan_slot
            );
            non_empty_slots.push(ScanHit {
                requested_slot: requested_scan_slot,
                slot: resolved_scan_slot,
                epoch: resolved_scan_slot / SLOTS_PER_EPOCH,
                pending_consolidations: pending_consolidations.len(),
            });
            if first_hit.is_none() {
                first_hit = Some((resolved_scan_slot, pending_consolidations.clone()));
            }
            if args
                .scan_hit_limit
                .is_some_and(|limit| non_empty_slots.len() >= limit)
            {
                println!(
                    "   Reached scan hit limit ({} non-empty states); stopping early",
                    non_empty_slots.len()
                );
                break;
            }
        }
    }

    let first_non_empty_slot = min_hit_slot(&non_empty_slots);
    let last_non_empty_slot = max_hit_slot(&non_empty_slots);

    let scan_window = Some(ScanWindow {
        start_slot: scan_start_slot,
        end_slot: scan_end_slot,
        start_epoch: scan_start_slot / SLOTS_PER_EPOCH,
        end_epoch: scan_end_slot / SLOTS_PER_EPOCH,
        slots_checked,
        scan_step_slots: args.scan_step_slots,
        scan_direction: args.scan_direction.as_str().to_string(),
        scan_hit_limit: args.scan_hit_limit,
        first_non_empty_slot,
        last_non_empty_slot,
        first_non_empty_epoch: first_non_empty_slot.map(|slot| slot / SLOTS_PER_EPOCH),
        last_non_empty_epoch: last_non_empty_slot.map(|slot| slot / SLOTS_PER_EPOCH),
        non_empty_slots,
    });

    if let Some((slot, pending_consolidations)) = first_hit {
        return Ok((
            slot,
            pending_consolidations,
            scan_window,
            None,
            finality.clone(),
        ));
    }

    println!("   No pending consolidations found in scan window\n");
    Ok((
        requested_slot,
        fallback_pending.unwrap_or_default(),
        scan_window,
        None,
        finality.clone(),
    ))
}

async fn watch_finalized_state(
    args: &Args,
    client: &BeaconClient,
    initial_finality: &FinalityCheckpoints,
) -> Result<(
    u64,
    Vec<PendingConsolidationJson>,
    WatchSummary,
    FinalityCheckpoints,
)> {
    anyhow::ensure!(
        args.watch_poll_seconds > 0,
        "watch_poll_seconds must be greater than zero"
    );
    validate_scan_hit_limit(args.watch_max_polls)?;

    println!(
        "👀 Watching finalized state every {}s for pending consolidations...",
        args.watch_poll_seconds
    );

    let mut polls = 0usize;

    loop {
        let finality = if polls == 0 {
            initial_finality.clone()
        } else {
            client
                .get_finality_checkpoints()
                .await
                .context("Failed to refresh finality checkpoints during watch")?
        };
        let finalized_slot = finality.finalized_epoch * SLOTS_PER_EPOCH;
        let pending_consolidations = client
            .get_pending_consolidations(&finalized_slot.to_string())
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch pending consolidations for finalized slot {} during watch",
                    finalized_slot
                )
            })?;
        polls += 1;

        println!(
            "   poll #{polls}: finalized slot {} (epoch {}) -> {} pending consolidations",
            finalized_slot,
            finality.finalized_epoch,
            pending_consolidations.len()
        );

        if !pending_consolidations.is_empty() {
            return Ok((
                finalized_slot,
                pending_consolidations,
                WatchSummary {
                    polls,
                    poll_interval_seconds: args.watch_poll_seconds,
                    max_polls: args.watch_max_polls,
                    resolved_slot: finalized_slot,
                    found_non_empty_state: true,
                },
                finality,
            ));
        }

        if args.watch_max_polls.is_some_and(|limit| polls >= limit) {
            return Ok((
                finalized_slot,
                pending_consolidations,
                WatchSummary {
                    polls,
                    poll_interval_seconds: args.watch_poll_seconds,
                    max_polls: args.watch_max_polls,
                    resolved_slot: finalized_slot,
                    found_non_empty_state: false,
                },
                finality,
            ));
        }

        sleep(Duration::from_secs(args.watch_poll_seconds)).await;
    }
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

fn has_scan_window(args: &Args) -> bool {
    args.scan_start_slot.is_some()
        || args.scan_end_slot.is_some()
        || args.scan_start_epoch.is_some()
        || args.scan_end_epoch.is_some()
        || args.scan_last_epochs.is_some()
}

fn has_non_default_scan_controls(args: &Args) -> bool {
    args.scan_step_slots != SLOTS_PER_EPOCH
        || args.scan_direction != ScanDirection::Forward
        || args.scan_hit_limit.is_some()
}

fn resolve_scan_window(args: &Args, finalized_slot: u64) -> Result<Option<(u64, u64)>> {
    let using_slot_range = args.scan_start_slot.is_some() || args.scan_end_slot.is_some();
    let using_epoch_range = args.scan_start_epoch.is_some() || args.scan_end_epoch.is_some();
    let using_recent_epochs = args.scan_last_epochs.is_some();

    anyhow::ensure!(
        [using_slot_range, using_epoch_range, using_recent_epochs]
            .into_iter()
            .filter(|enabled| *enabled)
            .count()
            <= 1,
        "slot-based, epoch-based, and recent-epoch scan flags are mutually exclusive"
    );

    if using_slot_range {
        let scan_start_slot = args
            .scan_start_slot
            .context("--scan-end-slot requires --scan-start-slot")?;
        let scan_end_slot = args
            .scan_end_slot
            .context("--scan-start-slot requires --scan-end-slot")?;
        return Ok(Some((scan_start_slot, scan_end_slot)));
    }

    if using_epoch_range {
        let scan_start_epoch = args
            .scan_start_epoch
            .context("--scan-end-epoch requires --scan-start-epoch")?;
        let scan_end_epoch = args
            .scan_end_epoch
            .context("--scan-start-epoch requires --scan-end-epoch")?;
        anyhow::ensure!(
            scan_start_epoch <= scan_end_epoch,
            "scan_start_epoch must be <= scan_end_epoch"
        );

        let scan_start_slot = scan_start_epoch
            .checked_mul(SLOTS_PER_EPOCH)
            .context("scan_start_epoch overflowed when converted to slot")?;
        let scan_end_slot = scan_end_epoch
            .checked_add(1)
            .and_then(|epoch| epoch.checked_mul(SLOTS_PER_EPOCH))
            .and_then(|slot| slot.checked_sub(1))
            .context("scan_end_epoch overflowed when converted to slot")?;
        return Ok(Some((scan_start_slot, scan_end_slot)));
    }

    if let Some(scan_last_epochs) = args.scan_last_epochs {
        anyhow::ensure!(
            scan_last_epochs > 0,
            "--scan-last-epochs must be greater than zero"
        );

        let lookback_slots = scan_last_epochs
            .checked_sub(1)
            .and_then(|epochs| epochs.checked_mul(SLOTS_PER_EPOCH))
            .context("scan_last_epochs overflowed when converted to slots")?;
        let scan_start_slot = finalized_slot.saturating_sub(lookback_slots);
        return Ok(Some((scan_start_slot, finalized_slot)));
    }

    Ok(None)
}

fn validate_scan_hit_limit(scan_hit_limit: Option<usize>) -> Result<()> {
    if let Some(limit) = scan_hit_limit {
        anyhow::ensure!(limit > 0, "scan_hit_limit must be greater than zero");
    }

    Ok(())
}

async fn fetch_pending_consolidations_at_or_before(
    client: &BeaconClient,
    requested_slot: u64,
    minimum_slot: u64,
) -> Result<(u64, Vec<PendingConsolidationJson>)> {
    let mut slot = requested_slot;

    loop {
        match client.get_pending_consolidations(&slot.to_string()).await {
            Ok(pending_consolidations) => return Ok((slot, pending_consolidations)),
            Err(BeaconClientError::StateNotFound(_)) => {
                match client.get_header(&slot.to_string()).await {
                    Ok(_) => {
                        anyhow::bail!(
                        "beacon header exists at slot {} but beacon state is unavailable; historical state lookups appear pruned on this node",
                        slot
                    );
                    }
                    Err(BeaconClientError::HeaderNotFound(_)) if slot > minimum_slot => {
                        slot -= 1;
                    }
                    Err(BeaconClientError::HeaderNotFound(_)) => {
                        anyhow::bail!(
                            "no beacon state found at or before slot {} within lower bound {}",
                            requested_slot,
                            minimum_slot
                        );
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn build_scan_slots(
    scan_start_slot: u64,
    scan_end_slot: u64,
    scan_step_slots: u64,
    scan_direction: ScanDirection,
    requested_slot: u64,
) -> Vec<u64> {
    let mut slots = Vec::new();
    let mut slot = scan_start_slot;
    while slot <= scan_end_slot {
        slots.push(slot);
        match slot.checked_add(scan_step_slots) {
            Some(next) if next > slot => slot = next,
            _ => break,
        }
    }

    if slots.last().copied() != Some(scan_end_slot) {
        slots.push(scan_end_slot);
    }

    if !slots.contains(&requested_slot) {
        slots.push(requested_slot);
    }

    slots.sort_unstable();
    slots.dedup();

    if scan_direction == ScanDirection::Reverse {
        slots.reverse();
    }

    slots
}

impl ScanDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
        }
    }
}

fn min_hit_slot(non_empty_slots: &[ScanHit]) -> Option<u64> {
    non_empty_slots.iter().map(|hit| hit.slot).min()
}

fn max_hit_slot(non_empty_slots: &[ScanHit]) -> Option<u64> {
    non_empty_slots.iter().map(|hit| hit.slot).max()
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

    #[test]
    fn scan_window_serializes_first_non_empty_slot() {
        let window = ScanWindow {
            start_slot: 10,
            end_slot: 20,
            start_epoch: 0,
            end_epoch: 1,
            slots_checked: 11,
            scan_step_slots: 16,
            scan_direction: "forward".to_string(),
            scan_hit_limit: Some(2),
            first_non_empty_slot: Some(14),
            last_non_empty_slot: Some(18),
            first_non_empty_epoch: Some(0),
            last_non_empty_epoch: Some(1),
            non_empty_slots: vec![
                ScanHit {
                    requested_slot: 14,
                    slot: 14,
                    epoch: 0,
                    pending_consolidations: 2,
                },
                ScanHit {
                    requested_slot: 18,
                    slot: 18,
                    epoch: 1,
                    pending_consolidations: 4,
                },
            ],
        };

        let json = serde_json::to_value(window).unwrap();
        assert_eq!(json["start_slot"], 10);
        assert_eq!(json["end_slot"], 20);
        assert_eq!(json["slots_checked"], 11);
        assert_eq!(json["scan_step_slots"], 16);
        assert_eq!(json["scan_direction"], "forward");
        assert_eq!(json["scan_hit_limit"], 2);
        assert_eq!(json["start_epoch"], 0);
        assert_eq!(json["end_epoch"], 1);
        assert_eq!(json["first_non_empty_slot"], 14);
        assert_eq!(json["last_non_empty_slot"], 18);
        assert_eq!(json["first_non_empty_epoch"], 0);
        assert_eq!(json["last_non_empty_epoch"], 1);
        assert_eq!(json["non_empty_slots"][0]["requested_slot"], 14);
        assert_eq!(json["non_empty_slots"][0]["slot"], 14);
        assert_eq!(json["non_empty_slots"][0]["epoch"], 0);
        assert_eq!(json["non_empty_slots"][0]["pending_consolidations"], 2);
        assert_eq!(json["non_empty_slots"][1]["requested_slot"], 18);
        assert_eq!(json["non_empty_slots"][1]["slot"], 18);
        assert_eq!(json["non_empty_slots"][1]["epoch"], 1);
        assert_eq!(json["non_empty_slots"][1]["pending_consolidations"], 4);
    }

    #[test]
    fn scan_window_serializes_null_epochs_when_no_hits_exist() {
        let window = ScanWindow {
            start_slot: 32,
            end_slot: 63,
            start_epoch: 2,
            end_epoch: 3,
            slots_checked: 2,
            scan_step_slots: 16,
            scan_direction: "reverse".to_string(),
            scan_hit_limit: None,
            first_non_empty_slot: None,
            last_non_empty_slot: None,
            first_non_empty_epoch: None,
            last_non_empty_epoch: None,
            non_empty_slots: vec![],
        };

        let json = serde_json::to_value(window).unwrap();
        assert_eq!(json["start_epoch"], 2);
        assert_eq!(json["end_epoch"], 3);
        assert!(json["first_non_empty_epoch"].is_null());
        assert!(json["last_non_empty_epoch"].is_null());
        assert!(json["non_empty_slots"].as_array().unwrap().is_empty());
    }

    #[test]
    fn validate_scan_hit_limit_rejects_zero() {
        let error = validate_scan_hit_limit(Some(0)).unwrap_err();
        assert!(error
            .to_string()
            .contains("scan_hit_limit must be greater than zero"));
    }

    #[test]
    fn validate_scan_hit_limit_accepts_none_and_positive_values() {
        validate_scan_hit_limit(None).unwrap();
        validate_scan_hit_limit(Some(3)).unwrap();
    }

    #[test]
    fn build_scan_slots_uses_step_and_includes_requested_slot() {
        let slots = build_scan_slots(100, 140, 16, ScanDirection::Forward, 133);
        assert_eq!(slots, vec![100, 116, 132, 133, 140]);
    }

    #[test]
    fn build_scan_slots_reverses_order_when_requested() {
        let slots = build_scan_slots(100, 140, 16, ScanDirection::Reverse, 132);
        assert_eq!(slots, vec![140, 132, 116, 100]);
    }

    #[test]
    fn hit_bounds_are_chronological_even_for_reverse_scan_order() {
        let hits = vec![
            ScanHit {
                requested_slot: 140,
                slot: 140,
                epoch: 8,
                pending_consolidations: 1,
            },
            ScanHit {
                requested_slot: 116,
                slot: 116,
                epoch: 7,
                pending_consolidations: 2,
            },
            ScanHit {
                requested_slot: 100,
                slot: 100,
                epoch: 6,
                pending_consolidations: 3,
            },
        ];

        assert_eq!(min_hit_slot(&hits), Some(100));
        assert_eq!(max_hit_slot(&hits), Some(140));
    }

    #[test]
    fn resolve_scan_window_accepts_slot_range() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-start-slot",
            "320",
            "--scan-end-slot",
            "400",
        ]);

        assert_eq!(resolve_scan_window(&args, 999).unwrap(), Some((320, 400)));
    }

    #[test]
    fn resolve_scan_window_converts_epoch_range_to_slots() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-start-epoch",
            "10",
            "--scan-end-epoch",
            "12",
        ]);

        assert_eq!(
            resolve_scan_window(&args, 999).unwrap(),
            Some((10 * SLOTS_PER_EPOCH, ((12 + 1) * SLOTS_PER_EPOCH) - 1))
        );
    }

    #[test]
    fn resolve_scan_window_rejects_mixed_slot_and_epoch_ranges() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-start-slot",
            "320",
            "--scan-end-slot",
            "400",
            "--scan-start-epoch",
            "10",
            "--scan-end-epoch",
            "12",
        ]);

        let error = resolve_scan_window(&args, 999).unwrap_err();
        assert!(error.to_string().contains(
            "slot-based, epoch-based, and recent-epoch scan flags are mutually exclusive"
        ));
    }

    #[test]
    fn resolve_scan_window_requires_epoch_pairs() {
        let args = Args::parse_from(["fetch-and-prove", "--scan-start-epoch", "10"]);

        let error = resolve_scan_window(&args, 999).unwrap_err();
        assert!(error
            .to_string()
            .contains("--scan-start-epoch requires --scan-end-epoch"));
    }

    #[test]
    fn resolve_scan_window_supports_recent_finalized_epochs() {
        let args = Args::parse_from(["fetch-and-prove", "--scan-last-epochs", "3"]);

        assert_eq!(
            resolve_scan_window(&args, 320).unwrap(),
            Some((320 - (2 * SLOTS_PER_EPOCH), 320))
        );
    }

    #[test]
    fn resolve_scan_window_rejects_zero_recent_epochs() {
        let args = Args::parse_from(["fetch-and-prove", "--scan-last-epochs", "0"]);

        let error = resolve_scan_window(&args, 320).unwrap_err();
        assert!(error
            .to_string()
            .contains("--scan-last-epochs must be greater than zero"));
    }

    #[test]
    fn resolve_scan_window_rejects_recent_epochs_with_explicit_ranges() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-last-epochs",
            "3",
            "--scan-start-epoch",
            "10",
            "--scan-end-epoch",
            "12",
        ]);

        let error = resolve_scan_window(&args, 320).unwrap_err();
        assert!(error.to_string().contains(
            "slot-based, epoch-based, and recent-epoch scan flags are mutually exclusive"
        ));
    }

    #[test]
    fn has_scan_window_detects_any_scan_flag() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-start-epoch",
            "10",
            "--scan-end-epoch",
            "12",
        ]);

        assert!(has_scan_window(&args));
    }

    #[test]
    fn has_scan_window_is_false_without_scan_flags() {
        let args = Args::parse_from(["fetch-and-prove"]);
        assert!(!has_scan_window(&args));
    }

    #[test]
    fn has_non_default_scan_controls_detects_modified_scan_knobs() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--scan-step-slots",
            "32",
            "--scan-direction",
            "reverse",
            "--scan-hit-limit",
            "1",
        ]);

        assert!(has_non_default_scan_controls(&args));
    }

    #[test]
    fn has_non_default_scan_controls_is_false_for_defaults() {
        let args = Args::parse_from(["fetch-and-prove"]);
        assert!(!has_non_default_scan_controls(&args));
    }

    #[test]
    fn watch_mode_reuses_positive_limit_validation() {
        validate_scan_hit_limit(Some(5)).unwrap();
    }

    #[test]
    fn scan_hit_serialization_is_stable() {
        let hit = ScanHit {
            requested_slot: 124,
            slot: 123,
            epoch: 7,
            pending_consolidations: 7,
        };

        let json = serde_json::to_value(hit).unwrap();
        assert_eq!(json["requested_slot"], 124);
        assert_eq!(json["slot"], 123);
        assert_eq!(json["epoch"], 7);
        assert_eq!(json["pending_consolidations"], 7);
    }

    fn sample_finality(epoch: u64) -> FinalityCheckpoints {
        FinalityCheckpoints {
            previous_justified_epoch: epoch.saturating_sub(2),
            current_justified_epoch: epoch.saturating_sub(1),
            finalized_epoch: epoch,
            finalized_root: [0x11; 32],
        }
    }

    #[tokio::test]
    async fn watch_finalized_state_stops_on_first_non_empty_poll() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let finalized_slot = 100 * SLOTS_PER_EPOCH;
        let response_json = r#"{
            "data": [
                {"source_index": "42", "target_index": "100"}
            ]
        }"#;

        Mock::given(method("GET"))
            .and(path(format!(
                "/eth/v1/beacon/states/{finalized_slot}/pending_consolidations"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let args = Args::parse_from([
            "fetch-and-prove",
            "--watch-finalized",
            "--watch-poll-seconds",
            "1",
        ]);
        let client = BeaconClient::new(mock_server.uri());

        let (resolved_slot, pending, watch_summary, finality) =
            watch_finalized_state(&args, &client, &sample_finality(100))
                .await
                .unwrap();

        assert_eq!(resolved_slot, finalized_slot);
        assert_eq!(finality.finalized_epoch, 100);
        assert_eq!(pending.len(), 1);
        assert!(watch_summary.found_non_empty_state);
        assert_eq!(watch_summary.polls, 1);
        assert_eq!(watch_summary.poll_interval_seconds, 1);
        assert_eq!(watch_summary.max_polls, None);
    }

    #[tokio::test]
    async fn watch_finalized_state_respects_max_polls_when_empty() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let finalized_slot = 100 * SLOTS_PER_EPOCH;
        let finality_json = format!(
            r#"{{
                "data": {{
                    "previous_justified": {{
                        "epoch": "98",
                        "root": "0x{root}"
                    }},
                    "current_justified": {{
                        "epoch": "99",
                        "root": "0x{root}"
                    }},
                    "finalized": {{
                        "epoch": "100",
                        "root": "0x{root}"
                    }}
                }}
            }}"#,
            root = "11".repeat(32)
        );

        Mock::given(method("GET"))
            .and(path(format!(
                "/eth/v1/beacon/states/{finalized_slot}/pending_consolidations"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data": []}"#))
            .expect(2)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
            .respond_with(ResponseTemplate::new(200).set_body_string(finality_json))
            .mount(&mock_server)
            .await;

        let args = Args::parse_from([
            "fetch-and-prove",
            "--watch-finalized",
            "--watch-poll-seconds",
            "1",
            "--watch-max-polls",
            "2",
        ]);
        let client = BeaconClient::new(mock_server.uri());

        let (resolved_slot, pending, watch_summary, finality) =
            watch_finalized_state(&args, &client, &sample_finality(100))
                .await
                .unwrap();

        assert_eq!(resolved_slot, finalized_slot);
        assert_eq!(finality.finalized_epoch, 100);
        assert!(pending.is_empty());
        assert!(!watch_summary.found_non_empty_state);
        assert_eq!(watch_summary.polls, 2);
        assert_eq!(watch_summary.max_polls, Some(2));
    }

    #[tokio::test]
    async fn watch_finalized_state_rejects_zero_poll_seconds() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--watch-finalized",
            "--watch-poll-seconds",
            "0",
        ]);
        let client = BeaconClient::new("http://127.0.0.1:1");

        let error = watch_finalized_state(&args, &client, &sample_finality(100))
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("watch_poll_seconds must be greater than zero"));
    }

    #[tokio::test]
    async fn resolve_target_state_rejects_watch_mode_with_non_default_scan_controls() {
        let args = Args::parse_from([
            "fetch-and-prove",
            "--watch-finalized",
            "--scan-direction",
            "reverse",
        ]);
        let client = BeaconClient::new("http://127.0.0.1:1");
        let finality = sample_finality(100);
        let finalized_slot = finality.finalized_epoch * SLOTS_PER_EPOCH;

        let error = resolve_target_state(&args, &client, finalized_slot, finalized_slot, &finality)
            .await
            .unwrap_err();
        assert!(error.to_string().contains(
            "--watch-finalized cannot be combined with scan-step-slots, scan-direction, or scan-hit-limit"
        ));
    }

    #[tokio::test]
    async fn resolve_target_state_rejects_watch_mode_with_non_finalized_state_id() {
        let args = Args::parse_from(["fetch-and-prove", "--watch-finalized", "--state-id", "head"]);
        let client = BeaconClient::new("http://127.0.0.1:1");
        let finality = sample_finality(100);
        let finalized_slot = finality.finalized_epoch * SLOTS_PER_EPOCH;

        let error = resolve_target_state(&args, &client, finalized_slot, finalized_slot, &finality)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("--watch-finalized currently requires --state-id finalized"));
    }

    #[tokio::test]
    async fn fetch_pending_consolidations_walks_back_to_previous_available_slot() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let response_json = r#"{
            "data": [
                {"source_index": "42", "target_index": "100"}
            ]
        }"#;

        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/120/pending_consolidations"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/120"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/119/pending_consolidations"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let (resolved_slot, pending) = fetch_pending_consolidations_at_or_before(&client, 120, 110)
            .await
            .unwrap();

        assert_eq!(resolved_slot, 119);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].source_index, 42);
    }

    #[tokio::test]
    async fn fetch_pending_consolidations_errors_when_no_state_exists_in_window() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        for slot in [120_u64, 119, 118] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/eth/v1/beacon/states/{slot}/pending_consolidations"
                )))
                .respond_with(ResponseTemplate::new(404))
                .mount(&mock_server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/eth/v1/beacon/headers/{slot}")))
                .respond_with(ResponseTemplate::new(404))
                .mount(&mock_server)
                .await;
        }

        let client = BeaconClient::new(mock_server.uri());
        let error = fetch_pending_consolidations_at_or_before(&client, 120, 118)
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("no beacon state found at or before slot 120 within lower bound 118"));
    }

    #[tokio::test]
    async fn fetch_pending_consolidations_detects_pruned_historical_state() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let header_json = r#"{
            "data": {
                "header": {
                    "message": {
                        "slot": "120",
                        "proposer_index": "42",
                        "parent_root": "0x0101010101010101010101010101010101010101010101010101010101010101",
                        "state_root": "0x0202020202020202020202020202020202020202020202020202020202020202",
                        "body_root": "0x0303030303030303030303030303030303030303030303030303030303030303"
                    }
                }
            }
        }"#;

        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/120/pending_consolidations"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/120"))
            .respond_with(ResponseTemplate::new(200).set_body_string(header_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let error = fetch_pending_consolidations_at_or_before(&client, 120, 100)
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("historical state lookups appear pruned on this node"));
    }
}
