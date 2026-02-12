//! State-level Sparse Proof Generator
//!
//! Generates Merkle proofs for beacon state fields using the sparse proof
//! approach. This works with any list limits (including gnosis's 2^40 validators)
//! without allocating full Merkle trees.

use crate::beacon_state::{BeaconBlockHeader, PendingConsolidation, Validator};
use crate::proof::{ConsolidationProofBundle, ProofError};
use crate::sparse_proof::{
    mix_in_length, prove_against_leaf_chunks, prove_small_container_field,
};
use ssz_rs::prelude::*;

/// Number of fields in the Electra BeaconState (constant across presets)
const BEACON_STATE_FIELD_COUNT: usize = 37;

/// Validators field index in BeaconState
const VALIDATORS_FIELD_INDEX: usize = 11;

/// Pending consolidations field index in BeaconState
const PENDING_CONSOLIDATIONS_FIELD_INDEX: usize = 36;

/// A sparse proof generator that builds proofs layer-by-layer.
pub struct StateProver {
    field_roots: Vec<[u8; 32]>,
    validator_hashes: Vec<[u8; 32]>,
    validator_count: usize,
    consolidation_hashes: Vec<[u8; 32]>,
    consolidation_count: usize,
    validators_tree_depth: u32,
    consolidations_tree_depth: u32,
    validators: Vec<Validator>,
    consolidations: Vec<PendingConsolidation>,
}

impl StateProver {
    /// Create a new StateProver from pre-computed field roots and element data.
    pub fn new(
        field_roots: Vec<[u8; 32]>,
        validators: Vec<Validator>,
        consolidations: Vec<PendingConsolidation>,
        validators_tree_depth: u32,
        consolidations_tree_depth: u32,
    ) -> Result<Self, ProofError> {
        if field_roots.len() != BEACON_STATE_FIELD_COUNT {
            return Err(ProofError::ProofGenerationFailed(format!(
                "Expected {} field roots, got {}",
                BEACON_STATE_FIELD_COUNT,
                field_roots.len()
            )));
        }

        let validator_hashes: Vec<[u8; 32]> = validators
            .iter()
            .map(|v| {
                let root = v.hash_tree_root().map_err(ProofError::MerkleizationError)?;
                Ok(root.into())
            })
            .collect::<Result<Vec<_>, ProofError>>()?;

        let consolidation_hashes: Vec<[u8; 32]> = consolidations
            .iter()
            .map(|c| {
                let root = c.hash_tree_root().map_err(ProofError::MerkleizationError)?;
                Ok(root.into())
            })
            .collect::<Result<Vec<_>, ProofError>>()?;

        let validator_count = validators.len();
        let consolidation_count = consolidations.len();

        Ok(Self {
            field_roots,
            validator_hashes,
            validator_count,
            consolidation_hashes,
            consolidation_count,
            validators_tree_depth,
            consolidations_tree_depth,
            validators,
            consolidations,
        })
    }

    /// Compute the state root from the field roots.
    pub fn compute_state_root(&self) -> [u8; 32] {
        let depth = 6u32;
        let (_proof, root) = prove_against_leaf_chunks(&self.field_roots, 0, depth);
        root
    }

    /// Generate a proof for pending_consolidations[i].source_index from state root.
    pub fn prove_consolidation_source_index(
        &self,
        consolidation_index: usize,
    ) -> Result<(Vec<[u8; 32]>, [u8; 32]), ProofError> {
        if consolidation_index >= self.consolidation_count {
            return Err(ProofError::ConsolidationIndexOutOfBounds(
                consolidation_index,
                self.consolidation_count,
            ));
        }

        let consolidation = &self.consolidations[consolidation_index];

        // Layer 1: source_index within PendingConsolidation (depth 1)
        let (inner_proof, inner_leaf, _) = prove_small_container_field(
            consolidation,
            &["source_index".into()],
        ).map_err(ProofError::MerkleizationError)?;

        // Layer 2: element[i] in consolidations data tree
        let (list_data_proof, _data_root) = prove_against_leaf_chunks(
            &self.consolidation_hashes,
            consolidation_index,
            self.consolidations_tree_depth,
        );

        // Layer 3: length mix-in sibling
        let mut length_bytes = [0u8; 32];
        length_bytes[..8].copy_from_slice(&(self.consolidation_count as u64).to_le_bytes());

        // Layer 4: pending_consolidations field in state container (depth 6)
        let (state_proof, _) = prove_against_leaf_chunks(
            &self.field_roots,
            PENDING_CONSOLIDATIONS_FIELD_INDEX,
            6,
        );

        let mut full_proof = inner_proof;
        full_proof.extend_from_slice(&list_data_proof);
        full_proof.push(length_bytes);
        full_proof.extend_from_slice(&state_proof);

        Ok((full_proof, inner_leaf))
    }

    /// Generate a proof for validators[i].withdrawal_credentials from state root.
    pub fn prove_validator_credentials(
        &self,
        validator_index: usize,
    ) -> Result<(Vec<[u8; 32]>, [u8; 32]), ProofError> {
        if validator_index >= self.validators.len() {
            return Err(ProofError::ValidatorIndexOutOfBounds(
                validator_index as u64,
                self.validators.len(),
            ));
        }

        let validator = &self.validators[validator_index];

        let (inner_proof, inner_leaf, _) = prove_small_container_field(
            validator,
            &["withdrawal_credentials".into()],
        ).map_err(ProofError::MerkleizationError)?;

        let (list_data_proof, _) = prove_against_leaf_chunks(
            &self.validator_hashes,
            validator_index,
            self.validators_tree_depth,
        );

        let mut length_bytes = [0u8; 32];
        length_bytes[..8].copy_from_slice(&(self.validator_count as u64).to_le_bytes());

        let (state_proof, _) = prove_against_leaf_chunks(
            &self.field_roots,
            VALIDATORS_FIELD_INDEX,
            6,
        );

        let mut full_proof = inner_proof;
        full_proof.extend_from_slice(&list_data_proof);
        full_proof.push(length_bytes);
        full_proof.extend_from_slice(&state_proof);

        Ok((full_proof, inner_leaf))
    }

    /// Generate a proof for validators[i].activation_epoch from state root.
    pub fn prove_validator_activation_epoch(
        &self,
        validator_index: usize,
    ) -> Result<(Vec<[u8; 32]>, [u8; 32]), ProofError> {
        if validator_index >= self.validators.len() {
            return Err(ProofError::ValidatorIndexOutOfBounds(
                validator_index as u64,
                self.validators.len(),
            ));
        }

        let validator = &self.validators[validator_index];

        let (inner_proof, inner_leaf, _) = prove_small_container_field(
            validator,
            &["activation_epoch".into()],
        ).map_err(ProofError::MerkleizationError)?;

        let (list_data_proof, _) = prove_against_leaf_chunks(
            &self.validator_hashes,
            validator_index,
            self.validators_tree_depth,
        );

        let mut length_bytes = [0u8; 32];
        length_bytes[..8].copy_from_slice(&(self.validator_count as u64).to_le_bytes());

        let (state_proof, _) = prove_against_leaf_chunks(
            &self.field_roots,
            VALIDATORS_FIELD_INDEX,
            6,
        );

        let mut full_proof = inner_proof;
        full_proof.extend_from_slice(&list_data_proof);
        full_proof.push(length_bytes);
        full_proof.extend_from_slice(&state_proof);

        Ok((full_proof, inner_leaf))
    }

    /// Generate full proof bundle from block root for a given consolidation.
    pub fn generate_full_proof_bundle(
        &self,
        header: &BeaconBlockHeader,
        consolidation_index: usize,
        beacon_timestamp: u64,
    ) -> Result<ConsolidationProofBundle, ProofError> {
        if consolidation_index >= self.consolidation_count {
            return Err(ProofError::ConsolidationIndexOutOfBounds(
                consolidation_index,
                self.consolidation_count,
            ));
        }

        let consolidation = &self.consolidations[consolidation_index];
        let source_index = consolidation.source_index as usize;

        if source_index >= self.validators.len() {
            return Err(ProofError::ValidatorIndexOutOfBounds(
                consolidation.source_index,
                self.validators.len(),
            ));
        }

        let validator = &self.validators[source_index];

        // Header proof: state_root is field 3 in header (depth 3)
        let (header_proof, _, _) = prove_small_container_field(
            header,
            &["state_root".into()],
        ).map_err(ProofError::MerkleizationError)?;

        // State-level proofs
        let (consolidation_state_proof, _) =
            self.prove_consolidation_source_index(consolidation_index)?;
        let (credentials_state_proof, _) =
            self.prove_validator_credentials(source_index)?;
        let (activation_state_proof, _) =
            self.prove_validator_activation_epoch(source_index)?;

        // Combine: state_proof + header_proof
        let mut full_consolidation_proof = consolidation_state_proof;
        full_consolidation_proof.extend_from_slice(&header_proof);

        let mut full_credentials_proof = credentials_state_proof;
        full_credentials_proof.extend_from_slice(&header_proof);

        let mut full_activation_proof = activation_state_proof;
        full_activation_proof.extend_from_slice(&header_proof);

        Ok(ConsolidationProofBundle {
            beacon_timestamp,
            consolidation_index: consolidation_index as u64,
            source_index: consolidation.source_index,
            activation_epoch: validator.activation_epoch,
            source_credentials: validator.withdrawal_credentials,
            proof_consolidation: full_consolidation_proof,
            proof_credentials: full_credentials_proof,
            proof_activation_epoch: full_activation_proof,
        })
    }
}

/// Compute the hash tree root of a list given element hashes and limits.
pub fn compute_list_root(
    element_hashes: &[[u8; 32]],
    tree_depth: u32,
    length: usize,
) -> [u8; 32] {
    let (_proof, data_root) = prove_against_leaf_chunks(element_hashes, 0, tree_depth);
    mix_in_length(data_root, length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon_state::MinimalBeaconState;
    use crate::gindex::GindexCalculator;

    fn make_validator(index: u8) -> Validator {
        let mut v = Validator::default();
        v.withdrawal_credentials[0] = 0x01;
        v.withdrawal_credentials[12..32].copy_from_slice(&[index; 20]);
        v.activation_epoch = 100 + index as u64;
        v.effective_balance = 32_000_000_000;
        v
    }

    fn state_prover_from_minimal(state: &MinimalBeaconState) -> StateProver {
        let field_roots = compute_minimal_state_field_roots(state);
        let validators_tree_depth = 10; // log2(1024)
        let consolidations_tree_depth = 6; // log2(64)

        StateProver::new(
            field_roots,
            state.validators.to_vec(),
            state.pending_consolidations.to_vec(),
            validators_tree_depth,
            consolidations_tree_depth,
        ).expect("should create prover")
    }

    fn compute_minimal_state_field_roots(state: &MinimalBeaconState) -> Vec<[u8; 32]> {
        vec![
            state.genesis_time.hash_tree_root().unwrap().into(),
            state.genesis_validators_root.hash_tree_root().unwrap().into(),
            state.slot.hash_tree_root().unwrap().into(),
            state.fork.hash_tree_root().unwrap().into(),
            state.latest_block_header.hash_tree_root().unwrap().into(),
            state.block_roots.hash_tree_root().unwrap().into(),
            state.state_roots.hash_tree_root().unwrap().into(),
            state.historical_roots.hash_tree_root().unwrap().into(),
            state.eth1_data.hash_tree_root().unwrap().into(),
            state.eth1_data_votes.hash_tree_root().unwrap().into(),
            state.eth1_deposit_index.hash_tree_root().unwrap().into(),
            state.validators.hash_tree_root().unwrap().into(),
            state.balances.hash_tree_root().unwrap().into(),
            state.randao_mixes.hash_tree_root().unwrap().into(),
            state.slashings.hash_tree_root().unwrap().into(),
            state.previous_epoch_participation.hash_tree_root().unwrap().into(),
            state.current_epoch_participation.hash_tree_root().unwrap().into(),
            state.justification_bits.hash_tree_root().unwrap().into(),
            state.previous_justified_checkpoint.hash_tree_root().unwrap().into(),
            state.current_justified_checkpoint.hash_tree_root().unwrap().into(),
            state.finalized_checkpoint.hash_tree_root().unwrap().into(),
            state.inactivity_scores.hash_tree_root().unwrap().into(),
            state.current_sync_committee.hash_tree_root().unwrap().into(),
            state.next_sync_committee.hash_tree_root().unwrap().into(),
            state.latest_execution_payload_header.hash_tree_root().unwrap().into(),
            state.next_withdrawal_index.hash_tree_root().unwrap().into(),
            state.next_withdrawal_validator_index.hash_tree_root().unwrap().into(),
            state.historical_summaries.hash_tree_root().unwrap().into(),
            state.deposit_requests_start_index.hash_tree_root().unwrap().into(),
            state.deposit_balance_to_consume.hash_tree_root().unwrap().into(),
            state.exit_balance_to_consume.hash_tree_root().unwrap().into(),
            state.earliest_exit_epoch.hash_tree_root().unwrap().into(),
            state.consolidation_balance_to_consume.hash_tree_root().unwrap().into(),
            state.earliest_consolidation_epoch.hash_tree_root().unwrap().into(),
            state.pending_deposits.hash_tree_root().unwrap().into(),
            state.pending_partial_withdrawals.hash_tree_root().unwrap().into(),
            state.pending_consolidations.hash_tree_root().unwrap().into(),
        ]
    }

    #[test]
    fn test_state_root_matches_ssz_rs() {
        let mut state = MinimalBeaconState::default();
        state.slot = 1000;
        state.genesis_time = 1234567890;

        for i in 0..5u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 2,
            target_index: 0,
        });

        let expected_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let prover = state_prover_from_minimal(&state);
        let computed_root = prover.compute_state_root();

        assert_eq!(computed_root, expected_root,
            "Sparse state root doesn't match ssz_rs state root");
    }

    #[test]
    fn test_consolidation_proof_verifies_against_state_root() {
        let mut state = MinimalBeaconState::default();
        state.slot = 500;

        for i in 0..5u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 3,
            target_index: 0,
        });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let prover = state_prover_from_minimal(&state);

        let (proof, leaf) = prover
            .prove_consolidation_source_index(0)
            .expect("should generate proof");

        let expected_leaf = {
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&3u64.to_le_bytes());
            b
        };
        assert_eq!(leaf, expected_leaf);

        let computed_gindex = GindexCalculator::concat_gindices(&[100, 2, 64, 2]);

        let state_root_node = Node::try_from(state_root.as_slice()).unwrap();
        let leaf_node = Node::try_from(leaf.as_slice()).unwrap();
        let branch: Vec<Node> = proof.iter().map(|b| Node::try_from(b.as_slice()).unwrap()).collect();

        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            leaf_node, &branch, computed_gindex as usize, state_root_node,
        ).expect("consolidation proof should verify against state root");
    }

    #[test]
    fn test_validator_credentials_proof_verifies() {
        let mut state = MinimalBeaconState::default();

        for i in 0..5u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 2,
            target_index: 0,
        });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let prover = state_prover_from_minimal(&state);

        let (proof, leaf) = prover
            .prove_validator_credentials(2)
            .expect("should generate proof");

        assert_eq!(leaf[0], 0x01);
        assert_eq!(&leaf[12..32], &[2u8; 20]);

        let computed_gindex = GindexCalculator::concat_gindices(&[75, 2, 1026, 9]);

        let state_root_node = Node::try_from(state_root.as_slice()).unwrap();
        let leaf_node = Node::try_from(leaf.as_slice()).unwrap();
        let branch: Vec<Node> = proof.iter().map(|b| Node::try_from(b.as_slice()).unwrap()).collect();

        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            leaf_node, &branch, computed_gindex as usize, state_root_node,
        ).expect("credentials proof should verify");
    }

    #[test]
    fn test_validator_activation_epoch_proof_verifies() {
        let mut state = MinimalBeaconState::default();

        for i in 0..3u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 1,
            target_index: 0,
        });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let prover = state_prover_from_minimal(&state);

        let (proof, leaf) = prover
            .prove_validator_activation_epoch(1)
            .expect("should generate proof");

        let expected_leaf = {
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&101u64.to_le_bytes());
            b
        };
        assert_eq!(leaf, expected_leaf);

        let computed_gindex = GindexCalculator::concat_gindices(&[75, 2, 1025, 13]);

        let state_root_node = Node::try_from(state_root.as_slice()).unwrap();
        let leaf_node = Node::try_from(leaf.as_slice()).unwrap();
        let branch: Vec<Node> = proof.iter().map(|b| Node::try_from(b.as_slice()).unwrap()).collect();

        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            leaf_node, &branch, computed_gindex as usize, state_root_node,
        ).expect("activation epoch proof should verify");
    }

    #[test]
    fn test_full_proof_bundle_verifies_against_block_root() {
        let mut state = MinimalBeaconState::default();
        state.slot = 1000;

        for i in 0..5u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 2,
            target_index: 0,
        });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let header = BeaconBlockHeader {
            slot: state.slot,
            proposer_index: 0,
            parent_root: [0u8; 32],
            state_root,
            body_root: [1u8; 32],
        };
        let block_root: [u8; 32] = header.hash_tree_root().unwrap().into();

        let prover = state_prover_from_minimal(&state);
        let bundle = prover
            .generate_full_proof_bundle(&header, 0, 1234567890)
            .expect("should generate bundle");

        assert_eq!(bundle.source_index, 2);
        assert_eq!(bundle.activation_epoch, 102);
        assert_eq!(bundle.source_credentials[0], 0x01);

        let block_root_node = Node::try_from(block_root.as_slice()).unwrap();

        // Verify consolidation proof
        let consolidation_gindex = GindexCalculator::concat_gindices(&[11, 100, 2, 64, 2]);
        let source_leaf = {
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&2u64.to_le_bytes());
            b
        };
        let source_node = Node::try_from(source_leaf.as_slice()).unwrap();
        let branch: Vec<Node> = bundle.proof_consolidation.iter()
            .map(|b| Node::try_from(b.as_slice()).unwrap()).collect();
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            source_node, &branch, consolidation_gindex as usize, block_root_node,
        ).expect("consolidation proof should verify");

        // Verify credentials proof
        let credentials_gindex = GindexCalculator::concat_gindices(&[11, 75, 2, 1026, 9]);
        let creds_node = Node::try_from(bundle.source_credentials.as_slice()).unwrap();
        let creds_branch: Vec<Node> = bundle.proof_credentials.iter()
            .map(|b| Node::try_from(b.as_slice()).unwrap()).collect();
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            creds_node, &creds_branch, credentials_gindex as usize, block_root_node,
        ).expect("credentials proof should verify");

        // Verify activation epoch proof
        let activation_gindex = GindexCalculator::concat_gindices(&[11, 75, 2, 1026, 13]);
        let activation_leaf = {
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&102u64.to_le_bytes());
            b
        };
        let activation_node = Node::try_from(activation_leaf.as_slice()).unwrap();
        let activation_branch: Vec<Node> = bundle.proof_activation_epoch.iter()
            .map(|b| Node::try_from(b.as_slice()).unwrap()).collect();
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            activation_node, &activation_branch, activation_gindex as usize, block_root_node,
        ).expect("activation epoch proof should verify");
    }

    #[test]
    fn test_proof_lengths_minimal_preset() {
        let mut state = MinimalBeaconState::default();

        for i in 0..3u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 1,
            target_index: 0,
        });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let header = BeaconBlockHeader {
            slot: 0, proposer_index: 0,
            parent_root: [0u8; 32], state_root, body_root: [0u8; 32],
        };

        let prover = state_prover_from_minimal(&state);
        let bundle = prover.generate_full_proof_bundle(&header, 0, 0).expect("should generate");

        // Consolidation: 1 (field) + 6 (data) + 1 (length) + 6 (state) + 3 (header) = 17
        assert_eq!(bundle.proof_consolidation.len(), 17);
        // Validator: 3 (field) + 10 (data) + 1 (length) + 6 (state) + 3 (header) = 23
        assert_eq!(bundle.proof_credentials.len(), 23);
        assert_eq!(bundle.proof_activation_epoch.len(), 23);
    }

    #[test]
    fn test_multiple_consolidations() {
        let mut state = MinimalBeaconState::default();

        for i in 0..10u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation { source_index: 3, target_index: 0 });
        state.pending_consolidations.push(PendingConsolidation { source_index: 7, target_index: 1 });
        state.pending_consolidations.push(PendingConsolidation { source_index: 5, target_index: 2 });

        let state_root: [u8; 32] = state.hash_tree_root().unwrap().into();
        let header = BeaconBlockHeader {
            slot: state.slot, proposer_index: 0,
            parent_root: [0u8; 32], state_root, body_root: [0u8; 32],
        };
        let block_root: [u8; 32] = header.hash_tree_root().unwrap().into();
        let block_root_node = Node::try_from(block_root.as_slice()).unwrap();

        let prover = state_prover_from_minimal(&state);

        for ci in 0..3 {
            let bundle = prover.generate_full_proof_bundle(&header, ci, 1000 + ci as u64)
                .expect("should generate bundle");

            let consolidation_gindex = GindexCalculator::concat_gindices(&[11, 100, 2, 64 + ci as u64, 2]);
            let source_leaf = {
                let mut b = [0u8; 32];
                b[..8].copy_from_slice(&bundle.source_index.to_le_bytes());
                b
            };
            let source_node = Node::try_from(source_leaf.as_slice()).unwrap();
            let branch: Vec<Node> = bundle.proof_consolidation.iter()
                .map(|b| Node::try_from(b.as_slice()).unwrap()).collect();

            ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
                source_node, &branch, consolidation_gindex as usize, block_root_node,
            ).unwrap_or_else(|e| panic!("consolidation {ci} proof failed: {e}"));
        }
    }

    #[test]
    fn test_cross_validate_with_ssz_rs_prove() {
        let mut state = MinimalBeaconState::default();
        state.slot = 42;

        for i in 0..3u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 1,
            target_index: 0,
        });

        // ssz_rs proof
        let path: &[PathElement] = &[
            "pending_consolidations".into(), 0usize.into(), "source_index".into(),
        ];
        let (ssz_proof, ssz_witness) = state.prove(path).expect("ssz_rs prove");
        let ssz_root: [u8; 32] = ssz_witness.into();
        let ssz_leaf: [u8; 32] = ssz_proof.leaf.into();
        let ssz_branch: Vec<[u8; 32]> = ssz_proof.branch.iter().map(|n| (*n).into()).collect();

        // sparse proof
        let prover = state_prover_from_minimal(&state);
        let (sparse_proof, sparse_leaf) = prover
            .prove_consolidation_source_index(0).expect("sparse prove");
        let sparse_root = prover.compute_state_root();

        assert_eq!(sparse_root, ssz_root, "state roots should match");
        assert_eq!(sparse_leaf, ssz_leaf, "leaves should match");
        assert_eq!(sparse_proof.len(), ssz_branch.len(), "proof lengths should match");

        for (i, (s, r)) in sparse_proof.iter().zip(ssz_branch.iter()).enumerate() {
            assert_eq!(s, r, "proof node {i} differs");
        }
    }

    #[test]
    fn test_cross_validate_credentials_with_ssz_rs() {
        let mut state = MinimalBeaconState::default();

        for i in 0..4u8 {
            state.validators.push(make_validator(i));
            state.balances.push(32_000_000_000);
        }

        state.pending_consolidations.push(PendingConsolidation {
            source_index: 2,
            target_index: 0,
        });

        let path: &[PathElement] = &[
            "validators".into(), 2usize.into(), "withdrawal_credentials".into(),
        ];
        let (ssz_proof, ssz_witness) = state.prove(path).expect("ssz_rs prove");
        let ssz_root: [u8; 32] = ssz_witness.into();
        let ssz_leaf: [u8; 32] = ssz_proof.leaf.into();
        let ssz_branch: Vec<[u8; 32]> = ssz_proof.branch.iter().map(|n| (*n).into()).collect();

        let prover = state_prover_from_minimal(&state);
        let (sparse_proof, sparse_leaf) = prover
            .prove_validator_credentials(2).expect("sparse prove");
        let sparse_root = prover.compute_state_root();

        assert_eq!(sparse_root, ssz_root);
        assert_eq!(sparse_leaf, ssz_leaf);
        assert_eq!(sparse_proof.len(), ssz_branch.len());

        for (i, (s, r)) in sparse_proof.iter().zip(ssz_branch.iter()).enumerate() {
            assert_eq!(s, r, "proof node {i} differs");
        }
    }
}
