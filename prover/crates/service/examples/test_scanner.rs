//! Test scanner against real Gnosis beacon node
use proof_gen::BeaconClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let beacon_url =
        std::env::var("BEACON_API_URL").unwrap_or_else(|_| "http://localhost:15052".to_string());

    let client = BeaconClient::new(&beacon_url);

    println!("🔍 Testing scanner against real Gnosis beacon node");
    println!("   Endpoint: {}\n", beacon_url);

    // Get head slot
    println!("📍 Fetching head slot...");
    let head_slot = client.get_head_slot().await?;
    println!("   Head slot: {}\n", head_slot);

    // Get finality
    println!("📊 Fetching finality checkpoints...");
    let checkpoints = client.get_finality_checkpoints().await?;
    println!("   Finalized epoch: {}", checkpoints.finalized_epoch);
    println!(
        "   Finalized root: 0x{}\n",
        hex::encode(checkpoints.finalized_root)
    );

    // Get pending consolidations
    let finalized_slot = checkpoints.finalized_epoch * 16; // Gnosis: 16 slots/epoch
    println!(
        "🔍 Fetching pending consolidations at slot {}...",
        finalized_slot
    );
    let consolidations = client
        .get_pending_consolidations(&finalized_slot.to_string())
        .await?;
    println!("   Found {} pending consolidations\n", consolidations.len());

    if !consolidations.is_empty() {
        println!("📋 Consolidations:");
        for (i, c) in consolidations.iter().enumerate() {
            println!(
                "   {}. source: {} → target: {}",
                i + 1,
                c.source_index,
                c.target_index
            );
        }
        println!();
    }

    println!("✅ Scanner test successful!");
    println!("   All Beacon API endpoints working correctly with real Gnosis chain data.");

    Ok(())
}
