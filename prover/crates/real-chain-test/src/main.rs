use anyhow::{Context, Result};
use proof_gen::beacon_client::BeaconClient;
use serde::Serialize;
use ssz_rs::HashTreeRoot;
use std::fs;

#[derive(Debug, Serialize)]
struct RealChainTestVector {
    description: String,
    source: String,
    slot: u64,
    beacon_timestamp: u64,
    block_root: String,
    state_root: String,
    validators_count: usize,
    consolidations_count: usize,
    claims: Vec<ClaimData>,
}

#[derive(Debug, Serialize)]
struct ClaimData {
    consolidation_index: u64,
    source_index: u64,
    target_index: u64,
    activation_epoch: u64,
    source_credentials: String,
    proof_consolidation: Vec<String>,
    proof_credentials: Vec<String>,
    proof_activation_epoch: Vec<String>,
    expected_recipient: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Fetching real Gnosis beacon chain data...\n");

    // Connect to Gnosis beacon node
    // Try public endpoint first, fallback to internal if available
    let beacon_url = std::env::var("GNOSIS_BEACON_URL")
        .unwrap_or_else(|_| "https://rpc.gnosischain.com/beacon".to_string());
    println!("Using beacon endpoint: {}\n", beacon_url);
    let client = BeaconClient::new(beacon_url.clone());

    // Get current finalized checkpoint
    println!("📍 Fetching finalized checkpoint...");
    let finality = client.get_finality_checkpoints().await
        .context("Failed to fetch finality checkpoints")?;
    
    let finalized_slot = finality.finalized_epoch * 16; // Gnosis: 16 slots per epoch
    println!("   Finalized epoch: {}", finality.finalized_epoch);
    println!("   Finalized slot: {}", finalized_slot);
    println!("   Finalized root: 0x{}\n", hex::encode(&finality.finalized_root));

    // Fetch the beacon block header at finalized slot
    println!("📦 Fetching beacon block header at slot {}...", finalized_slot);
    let block_id = format!("{}", finalized_slot);
    let header = client.get_header(&block_id).await
        .context("Failed to fetch beacon block header")?;
    
    let block_root = header.hash_tree_root()
        .map_err(|e| anyhow::anyhow!("Failed to compute block root: {:?}", e))?;
    
    println!("   State root: 0x{}", hex::encode(&header.state_root));
    println!("   Block root: 0x{}\n", hex::encode(&block_root));

    // Fetch the full beacon state in SSZ format
    println!("🌲 Fetching beacon state SSZ (this may take a moment)...");
    let state_id = format!("{}", finalized_slot);
    let state_ssz = client.get_state_ssz(&state_id).await
        .context("Failed to fetch state SSZ")?;
    
    println!("   State size: {} bytes ({:.2} MB)\n", state_ssz.len(), state_ssz.len() as f64 / 1_000_000.0);

    // Parse the SSZ state
    // Note: We need to extract just validators and pending_consolidations
    // Full deserialization of Electra BeaconState is complex, so we'll use a targeted approach
    
    println!("⚠️  Full BeaconState SSZ deserialization requires complete Electra schema");
    println!("    For now, we'll demonstrate the proof pipeline with the header data.\n");

    // Calculate beacon timestamp (Gnosis genesis: 1638993340, 5s slots)
    let gnosis_genesis_time = 1638993340u64;
    let beacon_timestamp = gnosis_genesis_time + (finalized_slot * 5);

    println!("📊 Summary:");
    println!("   Slot: {}", finalized_slot);
    println!("   Beacon timestamp: {}", beacon_timestamp);
    println!("   Block root: 0x{}", hex::encode(&block_root));
    println!("   State root: 0x{}", hex::encode(&header.state_root));
    println!("\n✅ Successfully fetched real Gnosis beacon chain data!");
    println!("\n📝 Next steps:");
    println!("   1. Implement full Electra BeaconState SSZ deserialization");
    println!("   2. Extract validators and pending_consolidations from state SSZ");
    println!("   3. Generate proofs for actual consolidations");
    println!("   4. Export as test vectors for devnet testing");

    // Save metadata to file
    let metadata = serde_json::json!({
        "description": "Real Gnosis beacon chain data snapshot",
        "beacon_node": beacon_url,
        "finalized_epoch": finality.finalized_epoch,
        "finalized_slot": finalized_slot,
        "beacon_timestamp": beacon_timestamp,
        "block_root": format!("0x{}", hex::encode(&block_root)),
        "state_root": format!("0x{}", hex::encode(&header.state_root)),
        "state_size_bytes": state_ssz.len(),
        "note": "Full BeaconState deserialization pending - requires complete Electra SSZ schema"
    });

    let output_path = "real_chain_snapshot.json";
    fs::write(output_path, serde_json::to_string_pretty(&metadata)?)?;
    println!("\n💾 Saved metadata to {}", output_path);

    Ok(())
}
