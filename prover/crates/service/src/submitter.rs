//! Transaction Submitter
//!
//! Submits consolidation reward claims to the smart contract.

use alloy::{
    network::EthereumWallet,
    primitives::{Address, FixedBytes, B256, U256},
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use anyhow::{Context, Result};
use proof_gen::ConsolidationProofBundle;
use tracing::{debug, info, instrument};

// Generate contract bindings from ABI
sol! {
    #[sol(rpc)]
    contract ConsolidationIncentives {
        function claimReward(
            uint64 beaconTimestamp,
            uint64 consolidationIndex,
            uint64 sourceIndex,
            uint64 activationEpoch,
            bytes32 sourceCredentials,
            bytes32[] calldata proofConsolidation,
            bytes32[] calldata proofCredentials,
            bytes32[] calldata proofActivationEpoch
        ) external;

        function rewarded(uint64 sourceIndex) external view returns (bool);
        function rewardAmount() external view returns (uint256);
        function maxEpoch() external view returns (uint64);
    }
}

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
    contract_address: Address,
    signer: Option<PrivateKeySigner>,
}

impl Submitter {
    /// Create a new submitter (read-only, no signer)
    pub fn new(config: SubmitterConfig) -> Result<Self> {
        let contract_address: Address = config
            .contract_address
            .parse()
            .context("Invalid contract address")?;

        Ok(Self {
            config,
            contract_address,
            signer: None,
        })
    }

    /// Create a submitter with a signer (can submit transactions)
    pub fn with_signer(config: SubmitterConfig) -> Result<Self> {
        let private_key = config
            .private_key
            .as_ref()
            .context("Private key required for signing")?;

        let contract_address: Address = config
            .contract_address
            .parse()
            .context("Invalid contract address")?;

        // Parse private key (handle with or without 0x prefix)
        let key_bytes = private_key.strip_prefix("0x").unwrap_or(private_key);
        let signer: PrivateKeySigner = key_bytes.parse().context("Invalid private key")?;

        info!(
            address = %signer.address(),
            "Submitter initialized with signer"
        );

        Ok(Self {
            config,
            contract_address,
            signer: Some(signer),
        })
    }

    /// Get the signer address (if configured)
    pub fn signer_address(&self) -> Option<Address> {
        self.signer.as_ref().map(|s| s.address())
    }

    /// Submit a consolidation reward claim
    ///
    /// # Errors
    /// Returns an error if:
    /// - Submitter not configured with signer
    /// - Gas price exceeds configured maximum
    /// - Transaction fails or reverts
    #[instrument(skip(self, proof), fields(source_index = proof.source_index))]
    pub async fn submit_claim(&self, proof: ConsolidationProofBundle) -> Result<B256> {
        let signer = self
            .signer
            .as_ref()
            .context("Submitter not configured with signer")?;

        // Build provider with wallet
        let wallet = EthereumWallet::from(signer.clone());
        let url: reqwest::Url = self.config.rpc_url.parse()?;
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(url);

        // Check current gas price
        let gas_price = provider.get_gas_price().await?;
        let max_gas_price_wei = U256::from(self.config.max_gas_price_gwei) * U256::from(1_000_000_000);
        if U256::from(gas_price) > max_gas_price_wei {
            anyhow::bail!(
                "Gas price {} gwei exceeds maximum {} gwei",
                gas_price / 1_000_000_000,
                self.config.max_gas_price_gwei
            );
        }

        // Create contract instance
        let contract = ConsolidationIncentives::new(self.contract_address, &provider);

        // Convert proof data to alloy types
        let proof_consolidation: Vec<FixedBytes<32>> = proof
            .proof_consolidation
            .iter()
            .map(|p| FixedBytes::from_slice(p))
            .collect();

        let proof_credentials: Vec<FixedBytes<32>> = proof
            .proof_credentials
            .iter()
            .map(|p| FixedBytes::from_slice(p))
            .collect();

        let proof_activation_epoch: Vec<FixedBytes<32>> = proof
            .proof_activation_epoch
            .iter()
            .map(|p| FixedBytes::from_slice(p))
            .collect();

        let source_credentials = FixedBytes::from_slice(&proof.source_credentials);

        info!(
            source_index = proof.source_index,
            consolidation_index = proof.consolidation_index,
            beacon_timestamp = proof.beacon_timestamp,
            activation_epoch = proof.activation_epoch,
            "Submitting reward claim"
        );

        // Build and send transaction
        let call = contract.claimReward(
            proof.beacon_timestamp,
            proof.consolidation_index,
            proof.source_index,
            proof.activation_epoch,
            source_credentials,
            proof_consolidation,
            proof_credentials,
            proof_activation_epoch,
        );

        let pending_tx = call.send().await.context("Failed to send transaction")?;
        let tx_hash = *pending_tx.tx_hash();

        info!(tx_hash = %tx_hash, "Transaction submitted");

        // Wait for confirmations if configured
        if self.config.confirmations > 0 {
            debug!(
                confirmations = self.config.confirmations,
                "Waiting for confirmations"
            );
            let receipt = pending_tx
                .with_required_confirmations(self.config.confirmations)
                .get_receipt()
                .await
                .context("Failed to get transaction receipt")?;

            if !receipt.status() {
                anyhow::bail!("Transaction reverted: {}", tx_hash);
            }

            info!(
                tx_hash = %tx_hash,
                gas_used = receipt.gas_used,
                "Transaction confirmed"
            );
        }

        Ok(tx_hash)
    }

    /// Check if a validator has already been rewarded
    #[instrument(skip(self))]
    pub async fn is_rewarded(&self, source_index: u64) -> Result<bool> {
        let url: reqwest::Url = self.config.rpc_url.parse()?;
        let provider = ProviderBuilder::new().connect_http(url);

        let contract = ConsolidationIncentives::new(self.contract_address, &provider);
        let rewarded: bool = contract.rewarded(source_index).call().await?;

        debug!(source_index, rewarded, "Checked reward status");
        Ok(rewarded)
    }

    /// Get the reward amount configured in the contract
    pub async fn get_reward_amount(&self) -> Result<U256> {
        let url: reqwest::Url = self.config.rpc_url.parse()?;
        let provider = ProviderBuilder::new().connect_http(url);

        let contract = ConsolidationIncentives::new(self.contract_address, &provider);
        let amount: U256 = contract.rewardAmount().call().await?;
        Ok(amount)
    }

    /// Get the max epoch configured in the contract
    pub async fn get_max_epoch(&self) -> Result<u64> {
        let url: reqwest::Url = self.config.rpc_url.parse()?;
        let provider = ProviderBuilder::new().connect_http(url);

        let contract = ConsolidationIncentives::new(self.contract_address, &provider);
        let epoch: u64 = contract.maxEpoch().call().await?;
        Ok(epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submitter_creation_readonly() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            private_key: None,
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::new(config);
        assert!(submitter.is_ok());
        assert!(submitter.unwrap().signer_address().is_none());
    }

    #[test]
    fn test_submitter_creation_with_signer() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            // Anvil's first default private key
            private_key: Some(
                "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string(),
            ),
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::with_signer(config);
        assert!(submitter.is_ok());
        let submitter = submitter.unwrap();

        // Should be the first Anvil account
        assert_eq!(
            submitter.signer_address().unwrap(),
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
                .parse::<Address>()
                .unwrap()
        );
    }

    #[test]
    fn test_submitter_creation_with_0x_prefix() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            private_key: Some(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string(),
            ),
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
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            private_key: None,
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::with_signer(config);
        assert!(submitter.is_err());
    }

    #[test]
    fn test_submitter_invalid_address() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "not_an_address".to_string(),
            private_key: None,
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::new(config);
        assert!(submitter.is_err());
    }

    #[test]
    fn test_submitter_invalid_private_key() {
        let config = SubmitterConfig {
            rpc_url: "http://localhost:8545".to_string(),
            contract_address: "0x0000000000000000000000000000000000000001".to_string(),
            private_key: Some("not_a_key".to_string()),
            max_gas_price_gwei: 100,
            confirmations: 1,
        };

        let submitter = Submitter::with_signer(config);
        assert!(submitter.is_err());
    }
}
