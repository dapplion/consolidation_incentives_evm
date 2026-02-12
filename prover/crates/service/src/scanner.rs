//! Beacon Chain Scanner
//!
//! Continuously monitors the beacon chain for new consolidations.

use crate::state::{AppState, ClaimStatus, ConsolidationRecord};
use anyhow::Result;
use proof_gen::BeaconClient;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, instrument, warn};

/// Scanner configuration
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Beacon node URL
    pub beacon_url: String,
    /// Polling interval
    pub poll_interval: Duration,
    /// Slots per epoch (Gnosis = 16)
    pub slots_per_epoch: u64,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            beacon_url: "http://localhost:5052".to_string(),
            poll_interval: Duration::from_secs(5),
            slots_per_epoch: 16,
        }
    }
}

/// Beacon chain scanner
pub struct Scanner {
    config: ScannerConfig,
    client: BeaconClient,
    state: AppState,
}

impl Scanner {
    /// Create a new scanner
    pub fn new(config: ScannerConfig, state: AppState) -> Self {
        let client = BeaconClient::new(&config.beacon_url);
        Self {
            config,
            client,
            state,
        }
    }

    /// Run the scanner loop
    #[instrument(skip(self))]
    pub async fn run(&self) -> Result<()> {
        info!("Starting beacon chain scanner");

        loop {
            if let Err(e) = self.poll_once().await {
                error!(error = %e, "Scanner poll failed");
                self.state.set_error(Some(e.to_string()));
            } else {
                self.state.set_error(None);
            }

            sleep(self.config.poll_interval).await;
        }
    }

    /// Single poll iteration
    async fn poll_once(&self) -> Result<()> {
        // Get current head
        let head_slot = self.client.get_head_slot().await?;
        self.state.set_head_slot(head_slot);

        // Get finality checkpoints
        let checkpoints = self.client.get_finality_checkpoints().await?;
        let finalized_epoch = checkpoints.finalized_epoch;

        // Calculate finalized slot
        let finalized_slot = finalized_epoch * self.config.slots_per_epoch;
        self.state.set_current_slot(finalized_slot);
        self.state.set_current_epoch(finalized_epoch);

        // TODO: Fetch state and extract pending_consolidations
        // This will be implemented when we have full SSZ deserialization

        Ok(())
    }

    /// Process new consolidations found in beacon state
    #[allow(dead_code)]
    fn process_consolidations(&self, consolidations: Vec<(u64, u64)>, epoch: u64) {
        for (source_index, target_index) in consolidations {
            // Skip if already tracked
            if self.state.get_consolidation(source_index).is_some() {
                continue;
            }

            info!(
                source = source_index,
                target = target_index,
                epoch = epoch,
                "New consolidation detected"
            );

            let record = ConsolidationRecord {
                source_index,
                target_index,
                epoch_seen: epoch,
                status: ClaimStatus::Detected,
                tx_hash: None,
                error: None,
            };

            self.state.upsert_consolidation(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.slots_per_epoch, 16);
        assert_eq!(config.poll_interval, Duration::from_secs(5));
    }
}
