//! Transaction Submitter
//!
//! Submits consolidation reward claims to the smart contract.

use anyhow::Result;
use proof_gen::ConsolidationProofBundle;
use tracing::{info, instrument};

/// Submitter configuration
#[derive(Debug, Clone)]
pub struct SubmitterConfig {
    /// Gnosis RPC URL
    pub rpc_url: String,
    /// Contract address
    pub contract_address: String,
    /// Max gas price in Gwei
    pub max_gas_price_gwei: u64,
}

/// Transaction submitter
pub struct Submitter {
    config: SubmitterConfig,
    // provider and signer will be added when implementing actual submission
}

impl Submitter {
    /// Create a new submitter
    pub fn new(config: SubmitterConfig) -> Self {
        Self { config }
    }

    /// Submit a consolidation reward claim
    #[instrument(skip(self, proof))]
    pub async fn submit_claim(&self, proof: ConsolidationProofBundle) -> Result<String> {
        info!(
            source_index = proof.source_index,
            consolidation_index = proof.consolidation_index,
            "Submitting reward claim"
        );

        // TODO: Implement actual transaction submission using alloy
        // This will:
        // 1. Encode the claimReward() call data
        // 2. Estimate gas
        // 3. Build and sign transaction
        // 4. Submit and wait for confirmation

        // Placeholder - return mock tx hash
        Ok(format!(
            "0x{:064x}",
            proof.source_index
        ))
    }

    /// Check if a validator has already been rewarded
    #[instrument(skip(self))]
    pub async fn is_rewarded(&self, source_index: u64) -> Result<bool> {
        // TODO: Call rewarded(sourceIndex) on contract
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_submitter_creation() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            max_gas_price_gwei: 100,
        };

        let _submitter = Submitter::new(config);
    }
}
