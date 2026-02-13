//! Transaction Submitter
//!
//! Submits consolidation reward claims to the smart contract.

use anyhow::{Context, Result};
use proof_gen::ConsolidationProofBundle;
use tracing::{info, instrument};

/// Submitter configuration
#[derive(Debug, Clone)]
pub struct SubmitterConfig {
    /// Gnosis RPC URL
    pub rpc_url: String,
    /// Contract address
    pub contract_address: String,
    /// Private key for signing transactions (hex without 0x prefix)
    pub private_key: Option<String>,
    /// Max gas price in Gwei
    pub max_gas_price_gwei: u64,
    /// Wait for confirmations (0 = don't wait)
    pub confirmations: u64,
}

/// Transaction submitter
pub struct Submitter {
    config: SubmitterConfig,
    has_signer: bool,
}

impl Submitter {
    /// Create a new submitter (read-only, no signer)
    pub fn new(config: SubmitterConfig) -> Result<Self> {
        Ok(Self {
            config,
            has_signer: false,
        })
    }

    /// Create a submitter with a signer (can submit transactions)
    pub fn with_signer(config: SubmitterConfig) -> Result<Self> {
        config
            .private_key
            .as_ref()
            .context("Private key required for signing")?;

        Ok(Self {
            config,
            has_signer: true,
        })
    }

    /// Submit a consolidation reward claim
    ///
    /// # Errors
    /// Returns an error if:
    /// - Submitter not configured with signer
    /// - Gas price exceeds configured maximum
    /// - Transaction fails or reverts
    ///
    /// # Implementation Status
    /// This is a stub implementation. Full transaction submission requires:
    /// 1. Contract ABI bindings via alloy sol! macro
    /// 2. Provider with wallet/signer integration
    /// 3. Gas estimation and nonce management
    /// 4. Transaction broadcasting and receipt polling
    ///
    /// The structure is ready for implementation when alloy API patterns are finalized.
    #[instrument(skip(self, proof))]
    pub async fn submit_claim(&self, proof: ConsolidationProofBundle) -> Result<String> {
        if !self.has_signer {
            anyhow::bail!("Submitter not configured with signer");
        }

        info!(
            source_index = proof.source_index,
            consolidation_index = proof.consolidation_index,
            "Would submit reward claim (stub)"
        );

        // TODO: Implement actual transaction submission using alloy
        // This will:
        // 1. Create provider with wallet from config
        // 2. Encode the claimReward() call data
        // 3. Check gas price against max_gas_price_gwei
        // 4. Estimate gas
        // 5. Build and sign transaction
        // 6. Submit and wait for confirmations

        // Placeholder - return mock tx hash
        Ok(format!("0x{:064x}", proof.source_index))
    }

    /// Check if a validator has already been rewarded
    ///
    /// # Implementation Status
    /// This is a stub implementation. Full implementation requires:
    /// 1. Contract ABI bindings via alloy sol! macro
    /// 2. Read-only provider for view calls
    /// 3. Contract address parsing and validation
    #[instrument(skip(self))]
    pub async fn is_rewarded(&self, _source_index: u64) -> Result<bool> {
        // TODO: Call rewarded(sourceIndex) on contract
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submitter_creation_readonly() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            private_key: None,
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::new(config);
        assert!(submitter.is_ok());
    }

    #[test]
    fn test_submitter_creation_with_signer() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            private_key: Some("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string()),
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::with_signer(config);
        assert!(submitter.is_ok());
    }

    #[test]
    fn test_submitter_creation_missing_private_key() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            private_key: None,
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::with_signer(config);
        assert!(submitter.is_err());
    }
}
