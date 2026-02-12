//! Merkle proof generation for consolidation incentives.
//!
//! This module provides the core proof generation logic, creating the three
//! proofs needed for a consolidation reward claim:
//! 1. Proof of `pending_consolidations[i].source_index`
//! 2. Proof of `validators[source].withdrawal_credentials`
//! 3. Proof of `validators[source].activation_epoch`

use crate::gindex::GindexCalculator;
use crate::types::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during proof generation.
#[derive(Error, Debug)]
pub enum ProofError {
    #[error("Consolidation index {0} out of bounds (max {1})")]
    ConsolidationIndexOutOfBounds(usize, usize),

    #[error("Source validator index {0} out of bounds (max {1})")]
    ValidatorIndexOutOfBounds(u64, usize),

    #[error("SSZ serialization error: {0}")]
    SszError(String),

    #[error("Proof generation failed: {0}")]
    ProofGenerationFailed(String),
}

/// A complete proof bundle for claiming a consolidation reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationProofBundle {
    /// Beacon timestamp for EIP-4788 lookup
    pub beacon_timestamp: u64,

    /// Index in the pending_consolidations list
    pub consolidation_index: u64,

    /// Source validator index
    pub source_index: u64,

    /// Source validator's activation epoch
    pub activation_epoch: u64,

    /// Source validator's withdrawal credentials
    #[serde(with = "hex::serde")]
    pub source_credentials: [u8; 32],

    /// Merkle proof for pending_consolidations[i].source_index
    #[serde(with = "proof_vec_serde")]
    pub proof_consolidation: Vec<[u8; 32]>,

    /// Merkle proof for validators[source].withdrawal_credentials
    #[serde(with = "proof_vec_serde")]
    pub proof_credentials: Vec<[u8; 32]>,

    /// Merkle proof for validators[source].activation_epoch
    #[serde(with = "proof_vec_serde")]
    pub proof_activation_epoch: Vec<[u8; 32]>,
}

impl ConsolidationProofBundle {
    /// Get the expected recipient address from withdrawal credentials.
    pub fn recipient_address(&self) -> Option<[u8; 20]> {
        let prefix = self.source_credentials[0];
        if prefix == 0x01 || prefix == 0x02 {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&self.source_credentials[12..32]);
            Some(addr)
        } else {
            None
        }
    }
}

/// Proof generator for consolidation incentives.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProofGenerator;

impl ProofGenerator {
    /// Create a new proof generator.
    pub fn new() -> Self {
        Self
    }

    /// Get the expected proof lengths for this preset.
    pub fn expected_proof_lengths() -> (u32, u32) {
        (
            GindexCalculator::consolidation_proof_length(),
            GindexCalculator::validator_proof_length(),
        )
    }

    /// Generate a proof bundle for a specific consolidation.
    ///
    /// Note: This is a placeholder that requires the full beacon state
    /// and ssz_rs proof generation. The actual implementation will use
    /// ssz_rs's Merkle proof generation.
    pub fn generate_proof_bundle(
        &self,
        _beacon_timestamp: u64,
        _consolidation_index: usize,
        _pending_consolidations: &[PendingConsolidation],
        _validators: &[Validator],
    ) -> Result<ConsolidationProofBundle, ProofError> {
        // TODO: Implement actual proof generation using ssz_rs
        // This requires building the full SSZ tree and computing proofs
        // from the merkle tree.
        Err(ProofError::ProofGenerationFailed(
            "Full implementation requires beacon state SSZ tree".to_string(),
        ))
    }
}

/// Custom serde for Vec<[u8; 32]> as hex strings
mod proof_vec_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(data: &Vec<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_strings: Vec<String> = data.iter().map(|h| format!("0x{}", hex::encode(h))).collect();
        hex_strings.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<[u8; 32]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_strings: Vec<String> = Vec::deserialize(deserializer)?;
        hex_strings
            .into_iter()
            .map(|s| {
                let s = s.strip_prefix("0x").unwrap_or(&s);
                let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
                if bytes.len() != 32 {
                    return Err(serde::de::Error::custom("expected 32 bytes"));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(arr)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_bundle_recipient_0x01() {
        let mut creds = [0u8; 32];
        creds[0] = 0x01;
        creds[12..32].copy_from_slice(&[0xab; 20]);

        let bundle = ConsolidationProofBundle {
            beacon_timestamp: 0,
            consolidation_index: 0,
            source_index: 0,
            activation_epoch: 0,
            source_credentials: creds,
            proof_consolidation: vec![],
            proof_credentials: vec![],
            proof_activation_epoch: vec![],
        };

        assert_eq!(bundle.recipient_address(), Some([0xab; 20]));
    }

    #[test]
    fn test_proof_bundle_recipient_bls() {
        let bundle = ConsolidationProofBundle {
            beacon_timestamp: 0,
            consolidation_index: 0,
            source_index: 0,
            activation_epoch: 0,
            source_credentials: [0u8; 32], // 0x00 prefix
            proof_consolidation: vec![],
            proof_credentials: vec![],
            proof_activation_epoch: vec![],
        };

        assert_eq!(bundle.recipient_address(), None);
    }

    #[test]
    fn test_proof_bundle_json_roundtrip() {
        let mut creds = [0u8; 32];
        creds[0] = 0x01;

        let bundle = ConsolidationProofBundle {
            beacon_timestamp: 12345,
            consolidation_index: 1,
            source_index: 42,
            activation_epoch: 100,
            source_credentials: creds,
            proof_consolidation: vec![[0xaa; 32], [0xbb; 32]],
            proof_credentials: vec![[0xcc; 32]],
            proof_activation_epoch: vec![[0xdd; 32]],
        };

        let json = serde_json::to_string(&bundle).unwrap();
        let decoded: ConsolidationProofBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.beacon_timestamp, bundle.beacon_timestamp);
        assert_eq!(decoded.source_index, bundle.source_index);
        assert_eq!(decoded.proof_consolidation, bundle.proof_consolidation);
    }

    #[test]
    #[cfg(all(feature = "gnosis", not(feature = "minimal")))]
    fn test_expected_proof_lengths_gnosis() {
        let (consolidation_len, validator_len) = ProofGenerator::expected_proof_lengths();
        assert_eq!(consolidation_len, 29);
        assert_eq!(validator_len, 53);
    }
}
