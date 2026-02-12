//! Merkle proof generation for consolidation incentives.
//!
//! This module provides the core proof generation logic, creating the three
//! proofs needed for a consolidation reward claim:
//! 1. Proof of `pending_consolidations[i].source_index`
//! 2. Proof of `validators[source].withdrawal_credentials`
//! 3. Proof of `validators[source].activation_epoch`

use crate::beacon_state::{MinimalBeaconState, BeaconBlockHeader};
use crate::gindex::GindexCalculator;
use serde::{Deserialize, Serialize};
use ssz_rs::prelude::*;
use thiserror::Error;

/// Convert ssz_rs Node to [u8; 32]
fn node_to_bytes(node: Node) -> [u8; 32] {
    node.0.into()
}

/// Convert [u8; 32] to ssz_rs Node
fn bytes_to_node(bytes: [u8; 32]) -> Node {
    Node::from(bytes)
}

/// Convert Vec<Node> to Vec<[u8; 32]>
fn nodes_to_bytes(nodes: Vec<Node>) -> Vec<[u8; 32]> {
    nodes.into_iter().map(node_to_bytes).collect()
}

/// Convert Vec<[u8; 32]> to Vec<Node>
fn bytes_to_nodes(bytes: &[[u8; 32]]) -> Vec<Node> {
    bytes.iter().map(|b| bytes_to_node(*b)).collect()
}

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

    #[error("Merkleization error: {0}")]
    MerkleizationError(#[from] MerkleizationError),
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

    /// Get the expected proof lengths for the production preset.
    pub fn expected_proof_lengths() -> (u32, u32) {
        (
            GindexCalculator::consolidation_proof_length(),
            GindexCalculator::validator_proof_length(),
        )
    }
    
    /// Get the expected proof lengths for the test state (MinimalBeaconState).
    pub fn test_proof_lengths() -> (u32, u32) {
        (
            GindexCalculator::test_consolidation_proof_length(),
            GindexCalculator::test_validator_proof_length(),
        )
    }

    /// Generate all three proofs for a consolidation from a beacon state.
    ///
    /// This generates proofs from the beacon state root (not block root) to:
    /// - pending_consolidations[consolidation_index].source_index
    /// - validators[source_index].withdrawal_credentials
    /// - validators[source_index].activation_epoch
    pub fn generate_proofs_from_state(
        state: &MinimalBeaconState,
        consolidation_index: usize,
    ) -> Result<StateProofBundle, ProofError> {
        // Validate consolidation index
        if consolidation_index >= state.pending_consolidations.len() {
            return Err(ProofError::ConsolidationIndexOutOfBounds(
                consolidation_index,
                state.pending_consolidations.len(),
            ));
        }

        let consolidation = &state.pending_consolidations[consolidation_index];
        let source_index = consolidation.source_index as usize;

        // Validate source validator index
        if source_index >= state.validators.len() {
            return Err(ProofError::ValidatorIndexOutOfBounds(
                consolidation.source_index,
                state.validators.len(),
            ));
        }

        let validator = &state.validators[source_index];

        // Generate proof for pending_consolidations[i].source_index
        let consolidation_path: &[PathElement] = &[
            "pending_consolidations".into(),
            consolidation_index.into(),
            "source_index".into(),
        ];
        let (proof_consolidation, state_root) = state.prove(consolidation_path)?;

        // Generate proof for validators[source].withdrawal_credentials
        let credentials_path: &[PathElement] = &[
            "validators".into(),
            source_index.into(),
            "withdrawal_credentials".into(),
        ];
        let (proof_credentials, _) = state.prove(credentials_path)?;

        // Generate proof for validators[source].activation_epoch
        let activation_path: &[PathElement] = &[
            "validators".into(),
            source_index.into(),
            "activation_epoch".into(),
        ];
        let (proof_activation, _) = state.prove(activation_path)?;

        Ok(StateProofBundle {
            state_root: node_to_bytes(state_root),
            consolidation_index: consolidation_index as u64,
            source_index: consolidation.source_index,
            activation_epoch: validator.activation_epoch,
            source_credentials: validator.withdrawal_credentials,
            proof_consolidation: nodes_to_bytes(proof_consolidation.branch),
            proof_credentials: nodes_to_bytes(proof_credentials.branch),
            proof_activation_epoch: nodes_to_bytes(proof_activation.branch),
            // Store leaf values for verification
            consolidation_source_leaf: node_to_bytes(proof_consolidation.leaf),
            credentials_leaf: node_to_bytes(proof_credentials.leaf),
            activation_epoch_leaf: node_to_bytes(proof_activation.leaf),
        })
    }

    /// Generate the full proof bundle including header wrapping.
    /// This creates proofs from block_root -> state_root -> leaf.
    pub fn generate_full_proof_bundle(
        header: &BeaconBlockHeader,
        state: &MinimalBeaconState,
        consolidation_index: usize,
        beacon_timestamp: u64,
    ) -> Result<ConsolidationProofBundle, ProofError> {
        // First get proofs from state root
        let state_proofs = Self::generate_proofs_from_state(state, consolidation_index)?;

        // Generate proof of state_root in header (field index 3 -> gindex 11)
        let state_root_path: &[PathElement] = &["state_root".into()];
        let (header_proof, _block_root) = header.prove(state_root_path)?;
        let header_branch = nodes_to_bytes(header_proof.branch);

        // Combine proofs: header_proof goes at the end (closer to root)
        // The full proof is: state_proof + header_proof
        let mut full_consolidation_proof = state_proofs.proof_consolidation.clone();
        full_consolidation_proof.extend(header_branch.iter().cloned());

        let mut full_credentials_proof = state_proofs.proof_credentials.clone();
        full_credentials_proof.extend(header_branch.iter().cloned());

        let mut full_activation_proof = state_proofs.proof_activation_epoch.clone();
        full_activation_proof.extend(header_branch.iter().cloned());

        Ok(ConsolidationProofBundle {
            beacon_timestamp,
            consolidation_index: state_proofs.consolidation_index,
            source_index: state_proofs.source_index,
            activation_epoch: state_proofs.activation_epoch,
            source_credentials: state_proofs.source_credentials,
            proof_consolidation: full_consolidation_proof,
            proof_credentials: full_credentials_proof,
            proof_activation_epoch: full_activation_proof,
        })
    }

    /// Verify that a proof bundle is valid against a block root using test state gindices.
    /// 
    /// This uses the test state tree depths (smaller than production).
    pub fn verify_proof_bundle_test(
        bundle: &ConsolidationProofBundle,
        block_root: [u8; 32],
    ) -> Result<(), ProofError> {
        let block_root_node = bytes_to_node(block_root);
        
        // Verify consolidation proof using test gindex
        let consolidation_gindex = GindexCalculator::test_consolidation_source_gindex(bundle.consolidation_index);
        let consolidation_leaf = bytes_to_node(ssz_u64_to_bytes32(bundle.source_index));
        let consolidation_branch = bytes_to_nodes(&bundle.proof_consolidation);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            consolidation_leaf,
            &consolidation_branch,
            consolidation_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Consolidation proof invalid: {e}")))?;

        // Verify credentials proof using test gindex
        let credentials_gindex = GindexCalculator::test_validator_credentials_gindex(bundle.source_index);
        let credentials_leaf = bytes_to_node(bundle.source_credentials);
        let credentials_branch = bytes_to_nodes(&bundle.proof_credentials);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            credentials_leaf,
            &credentials_branch,
            credentials_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Credentials proof invalid: {e}")))?;

        // Verify activation epoch proof using test gindex
        let activation_gindex = GindexCalculator::test_validator_activation_epoch_gindex(bundle.source_index);
        let activation_leaf = bytes_to_node(ssz_u64_to_bytes32(bundle.activation_epoch));
        let activation_branch = bytes_to_nodes(&bundle.proof_activation_epoch);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            activation_leaf,
            &activation_branch,
            activation_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Activation epoch proof invalid: {e}")))?;

        Ok(())
    }

    /// Verify that a proof bundle is valid against a block root using production gindices.
    pub fn verify_proof_bundle(
        bundle: &ConsolidationProofBundle,
        block_root: [u8; 32],
    ) -> Result<(), ProofError> {
        let block_root_node = bytes_to_node(block_root);
        
        // Verify consolidation proof
        let consolidation_gindex = GindexCalculator::consolidation_source_gindex(bundle.consolidation_index);
        let consolidation_leaf = bytes_to_node(ssz_u64_to_bytes32(bundle.source_index));
        let consolidation_branch = bytes_to_nodes(&bundle.proof_consolidation);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            consolidation_leaf,
            &consolidation_branch,
            consolidation_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Consolidation proof invalid: {e}")))?;

        // Verify credentials proof
        let credentials_gindex = GindexCalculator::validator_credentials_gindex(bundle.source_index);
        let credentials_leaf = bytes_to_node(bundle.source_credentials);
        let credentials_branch = bytes_to_nodes(&bundle.proof_credentials);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            credentials_leaf,
            &credentials_branch,
            credentials_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Credentials proof invalid: {e}")))?;

        // Verify activation epoch proof
        let activation_gindex = GindexCalculator::validator_activation_epoch_gindex(bundle.source_index);
        let activation_leaf = bytes_to_node(ssz_u64_to_bytes32(bundle.activation_epoch));
        let activation_branch = bytes_to_nodes(&bundle.proof_activation_epoch);
        
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            activation_leaf,
            &activation_branch,
            activation_gindex as usize,
            block_root_node,
        ).map_err(|e| ProofError::ProofGenerationFailed(format!("Activation epoch proof invalid: {e}")))?;

        Ok(())
    }
}

/// Intermediate proof bundle from state root (without header wrapping)
#[derive(Debug, Clone)]
pub struct StateProofBundle {
    pub state_root: [u8; 32],
    pub consolidation_index: u64,
    pub source_index: u64,
    pub activation_epoch: u64,
    pub source_credentials: [u8; 32],
    pub proof_consolidation: Vec<[u8; 32]>,
    pub proof_credentials: Vec<[u8; 32]>,
    pub proof_activation_epoch: Vec<[u8; 32]>,
    pub consolidation_source_leaf: [u8; 32],
    pub credentials_leaf: [u8; 32],
    pub activation_epoch_leaf: [u8; 32],
}

/// Convert a u64 to SSZ little-endian bytes32 (leaf format)
fn ssz_u64_to_bytes32(value: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&value.to_le_bytes());
    bytes
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
    use crate::beacon_state::{Validator, PendingConsolidation};

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
    fn test_ssz_u64_to_bytes32() {
        let bytes = ssz_u64_to_bytes32(42);
        assert_eq!(bytes[0], 42);
        assert_eq!(bytes[1..8], [0u8; 7]);
        assert_eq!(bytes[8..32], [0u8; 24]);
        
        let bytes_max = ssz_u64_to_bytes32(u64::MAX);
        assert_eq!(&bytes_max[..8], &u64::MAX.to_le_bytes());
    }

    #[test]
    #[cfg(all(feature = "gnosis", not(feature = "minimal")))]
    fn test_expected_proof_lengths_gnosis() {
        let (consolidation_len, validator_len) = ProofGenerator::expected_proof_lengths();
        assert_eq!(consolidation_len, 29);
        assert_eq!(validator_len, 53);
    }
    
    #[test]
    fn test_expected_proof_lengths_test_state() {
        let (consolidation_len, validator_len) = ProofGenerator::test_proof_lengths();
        // Test state: header (3) + state (6) + list (1) + data + field
        // Consolidation: 3 + 6 + 1 + 6 + 1 = 17
        // Validator: 3 + 6 + 1 + 10 + 3 = 23
        assert_eq!(consolidation_len, 17);
        assert_eq!(validator_len, 23);
    }

    #[test]
    fn test_generate_proofs_from_state() {
        // Create a state with test data
        let mut state = MinimalBeaconState::default();
        
        // Add some validators
        for i in 0..5u8 {
            let mut validator = Validator::default();
            validator.withdrawal_credentials[0] = 0x01;
            validator.withdrawal_credentials[12..32].copy_from_slice(&[(i + 1); 20]);
            validator.activation_epoch = 100 + i as u64;
            validator.effective_balance = 32_000_000_000;
            state.validators.push(validator);
            state.balances.push(32_000_000_000);
        }
        
        // Add some consolidations
        state.pending_consolidations.push(PendingConsolidation {
            source_index: 2,
            target_index: 0,
        });
        
        // Generate proofs for consolidation 0
        let result = ProofGenerator::generate_proofs_from_state(&state, 0);
        assert!(result.is_ok(), "Failed to generate proofs: {:?}", result.err());
        
        let proofs = result.unwrap();
        assert_eq!(proofs.source_index, 2);
        assert_eq!(proofs.activation_epoch, 102);
        assert_eq!(proofs.source_credentials[0], 0x01);
        
        // Proofs should have some content
        assert!(!proofs.proof_consolidation.is_empty());
        assert!(!proofs.proof_credentials.is_empty());
        assert!(!proofs.proof_activation_epoch.is_empty());
        
        // Verify the state root is correct
        let computed_state_root: [u8; 32] = state.hash_tree_root().expect("hash state").into();
        assert_eq!(proofs.state_root, computed_state_root);
    }

    #[test]
    fn test_generate_proofs_out_of_bounds() {
        let state = MinimalBeaconState::default();
        
        // Should fail - no consolidations
        let result = ProofGenerator::generate_proofs_from_state(&state, 0);
        assert!(matches!(result, Err(ProofError::ConsolidationIndexOutOfBounds(0, 0))));
    }

    #[test]
    fn test_full_proof_bundle_generation() {
        // Create a state with test data
        let mut state = MinimalBeaconState::default();
        state.slot = 1000;
        
        // Add validators
        for i in 0..3u8 {
            let mut validator = Validator::default();
            validator.withdrawal_credentials[0] = 0x01;
            validator.withdrawal_credentials[31] = i + 1;
            validator.activation_epoch = 50 + i as u64;
            state.validators.push(validator);
            state.balances.push(32_000_000_000);
        }
        
        // Add a consolidation
        state.pending_consolidations.push(PendingConsolidation {
            source_index: 1,
            target_index: 0,
        });
        
        // Create header with correct state root
        let state_root_bytes: [u8; 32] = state.hash_tree_root().expect("hash state").into();
        let header = BeaconBlockHeader {
            slot: state.slot,
            proposer_index: 0,
            parent_root: [0u8; 32],
            state_root: state_root_bytes,
            body_root: [1u8; 32],
        };
        
        // Generate full proof bundle
        let result = ProofGenerator::generate_full_proof_bundle(
            &header,
            &state,
            0,
            1234567890,
        );
        
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let bundle = result.unwrap();
        
        assert_eq!(bundle.beacon_timestamp, 1234567890);
        assert_eq!(bundle.source_index, 1);
        assert_eq!(bundle.activation_epoch, 51);
        
        // Full proofs should have content
        assert!(!bundle.proof_consolidation.is_empty());
        assert!(!bundle.proof_credentials.is_empty());
        assert!(!bundle.proof_activation_epoch.is_empty());
        
        // Get expected proof lengths for test state
        let (expected_consolidation_len, expected_validator_len) = ProofGenerator::test_proof_lengths();
        
        // Verify proof lengths match expectations
        assert_eq!(bundle.proof_consolidation.len(), expected_consolidation_len as usize,
            "Consolidation proof length mismatch");
        assert_eq!(bundle.proof_credentials.len(), expected_validator_len as usize,
            "Credentials proof length mismatch");
        assert_eq!(bundle.proof_activation_epoch.len(), expected_validator_len as usize,
            "Activation epoch proof length mismatch");
        
        // Verify the proof bundle is valid
        let block_root: [u8; 32] = header.hash_tree_root().expect("hash header").into();
        let verify_result = ProofGenerator::verify_proof_bundle_test(&bundle, block_root);
        assert!(verify_result.is_ok(), "Proof verification failed: {:?}", verify_result.err());
    }
    
    #[test]
    fn test_proof_verification_with_wrong_block_root() {
        // Create a state with test data
        let mut state = MinimalBeaconState::default();
        
        let mut validator = Validator::default();
        validator.withdrawal_credentials[0] = 0x01;
        validator.activation_epoch = 100;
        state.validators.push(validator);
        state.balances.push(32_000_000_000);
        
        state.pending_consolidations.push(PendingConsolidation {
            source_index: 0,
            target_index: 0,
        });
        
        let state_root_bytes: [u8; 32] = state.hash_tree_root().expect("hash state").into();
        let header = BeaconBlockHeader {
            slot: 1000,
            proposer_index: 0,
            parent_root: [0u8; 32],
            state_root: state_root_bytes,
            body_root: [1u8; 32],
        };
        
        let bundle = ProofGenerator::generate_full_proof_bundle(
            &header,
            &state,
            0,
            1234567890,
        ).unwrap();
        
        // Try to verify with a wrong block root
        let wrong_root = [0xaa; 32];
        let result = ProofGenerator::verify_proof_bundle_test(&bundle, wrong_root);
        assert!(result.is_err(), "Should fail with wrong block root");
    }
}
