//! Full Electra BeaconState SSZ Type
//!
//! Defines the complete BeaconState structure with all 37 fields required for Electra.
//! This is necessary for generating valid Merkle proofs.
//!
//! ## Test State Limits
//!
//! For test vector generation, we use small limits that allow in-memory proof generation:
//! - Validators: 2^10 = 1024 (tree depth 10)
//! - Pending consolidations: 2^6 = 64 (tree depth 6)
//!
//! These produce proofs with different lengths than gnosis mainnet (which uses 2^40 validators
//! and 2^18 pending consolidations). The Solidity test vectors generator will account for this.

use ssz_rs::prelude::*;

/// Checkpoint for fork choice
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct Checkpoint {
    pub epoch: u64,
    pub root: [u8; 32],
}

/// Attestation data
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct AttestationData {
    pub slot: u64,
    pub index: u64,
    pub beacon_block_root: [u8; 32],
    pub source: Checkpoint,
    pub target: Checkpoint,
}

/// Pending attestation (used in some forks)
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct PendingAttestation {
    pub aggregation_bits: Bitlist<2048>, // MAX_VALIDATORS_PER_COMMITTEE
    pub data: AttestationData,
    pub inclusion_delay: u64,
    pub proposer_index: u64,
}

/// Eth1 deposit data
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct Eth1Data {
    pub deposit_root: [u8; 32],
    pub deposit_count: u64,
    pub block_hash: [u8; 32],
}

/// Fork data
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct Fork {
    pub previous_version: [u8; 4],
    pub current_version: [u8; 4],
    pub epoch: u64,
}

/// Block header (sync committee style)
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: [u8; 32],
    pub state_root: [u8; 32],
    pub body_root: [u8; 32],
}

/// Validator record
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct Validator {
    pub pubkey: Vector<u8, 48>,
    pub withdrawal_credentials: [u8; 32],
    pub effective_balance: u64,
    pub slashed: bool,
    pub activation_eligibility_epoch: u64,
    pub activation_epoch: u64,
    pub exit_epoch: u64,
    pub withdrawable_epoch: u64,
}

/// Pending consolidation entry
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct PendingConsolidation {
    pub source_index: u64,
    pub target_index: u64,
}

/// Pending deposit entry (Electra)
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct PendingDeposit {
    pub pubkey: Vector<u8, 48>,
    pub withdrawal_credentials: [u8; 32],
    pub amount: u64,
    pub signature: Vector<u8, 96>,
    pub slot: u64,
}

/// Pending partial withdrawal (Electra)
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct PendingPartialWithdrawal {
    pub index: u64,
    pub amount: u64,
    pub withdrawable_epoch: u64,
}

/// Historical summary
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct HistoricalSummary {
    pub block_summary_root: [u8; 32],
    pub state_summary_root: [u8; 32],
}

/// Sync committee (Altair+)
#[derive(Debug, Clone, PartialEq, Eq, SimpleSerialize)]
pub struct SyncCommittee {
    pub pubkeys: Vector<Vector<u8, 48>, 512>, // SYNC_COMMITTEE_SIZE
    pub aggregate_pubkey: Vector<u8, 48>,
}

impl Default for SyncCommittee {
    fn default() -> Self {
        Self {
            pubkeys: Default::default(),
            aggregate_pubkey: Vector::default(),
        }
    }
}

// ============================================================================
// Test Constants - Small limits for in-memory proof generation
// ============================================================================

/// Validator limit for test state: 2^10 = 1024
/// Tree depth: 10
pub const TEST_VALIDATOR_LIMIT: usize = 1024;

/// Pending consolidations limit for test state: 2^6 = 64
/// Tree depth: 6
pub const TEST_PENDING_CONSOLIDATIONS_LIMIT: usize = 64;

/// Pending deposits limit: 2^8 = 256
pub const TEST_PENDING_DEPOSITS_LIMIT: usize = 256;

/// Pending withdrawals limit: 2^8 = 256
pub const TEST_PENDING_WITHDRAWALS_LIMIT: usize = 256;

/// Historical roots/summaries limit: 2^10 = 1024
pub const TEST_HISTORY_LIMIT: usize = 1024;

/// Eth1 data votes limit: 32
pub const TEST_ETH1_VOTES_LIMIT: usize = 32;

// ============================================================================
// Test BeaconState - Small limits for test vector generation
// ============================================================================

/// Minimal execution payload header (placeholder)
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct ExecutionPayloadHeaderMinimal {
    pub parent_hash: [u8; 32],
    pub fee_recipient: [u8; 20],
    pub state_root: [u8; 32],
    pub receipts_root: [u8; 32],
    pub logs_bloom: Vector<u8, 256>,
    pub prev_randao: [u8; 32],
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: List<u8, 32>,
    pub base_fee_per_gas: U256,
    pub block_hash: [u8; 32],
    pub transactions_root: [u8; 32],
    pub withdrawals_root: [u8; 32],
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
}

/// Test BeaconState with small limits
///
/// This state type uses small list limits suitable for in-memory test vector generation.
/// The tree structure matches the Electra fork with all 37 fields.
///
/// **Important**: The proof lengths from this state differ from mainnet/gnosis:
/// - Validators tree depth: 10 (vs 40 on mainnet)  
/// - Pending consolidations tree depth: 6 (vs 18 on gnosis)
#[derive(Debug, Clone, PartialEq, Eq, SimpleSerialize)]
pub struct MinimalBeaconState {
    // Field 0: Genesis time
    pub genesis_time: u64,
    // Field 1: Genesis validators root
    pub genesis_validators_root: [u8; 32],
    // Field 2: Slot number
    pub slot: u64,
    // Field 3: Fork info
    pub fork: Fork,
    // Field 4: Latest block header
    pub latest_block_header: BeaconBlockHeader,
    // Field 5: Block roots (SLOTS_PER_HISTORICAL_ROOT = 64 for minimal)
    pub block_roots: Vector<[u8; 32], 64>,
    // Field 6: State roots
    pub state_roots: Vector<[u8; 32], 64>,
    // Field 7: Historical roots (frozen in Capella)
    pub historical_roots: List<[u8; 32], 1024>,
    // Field 8: Eth1 data
    pub eth1_data: Eth1Data,
    // Field 9: Eth1 data votes
    pub eth1_data_votes: List<Eth1Data, 32>,
    // Field 10: Eth1 deposit index
    pub eth1_deposit_index: u64,
    // Field 11: Validators (small limit for testing: 2^10 = 1024)
    pub validators: List<Validator, 1024>,
    // Field 12: Balances
    pub balances: List<u64, 1024>,
    // Field 13: RANDAO mixes
    pub randao_mixes: Vector<[u8; 32], 64>,
    // Field 14: Slashings
    pub slashings: Vector<u64, 64>,
    // Field 15: Previous epoch participation
    pub previous_epoch_participation: List<u8, 1024>,
    // Field 16: Current epoch participation
    pub current_epoch_participation: List<u8, 1024>,
    // Field 17: Justification bits
    pub justification_bits: Bitvector<4>,
    // Field 18: Previous justified checkpoint
    pub previous_justified_checkpoint: Checkpoint,
    // Field 19: Current justified checkpoint
    pub current_justified_checkpoint: Checkpoint,
    // Field 20: Finalized checkpoint
    pub finalized_checkpoint: Checkpoint,
    // Field 21: Inactivity scores
    pub inactivity_scores: List<u64, 1024>,
    // Field 22: Current sync committee
    pub current_sync_committee: SyncCommittee,
    // Field 23: Next sync committee
    pub next_sync_committee: SyncCommittee,
    // Field 24: Latest execution payload header
    pub latest_execution_payload_header: ExecutionPayloadHeaderMinimal,
    // Field 25: Next withdrawal index
    pub next_withdrawal_index: u64,
    // Field 26: Next withdrawal validator index
    pub next_withdrawal_validator_index: u64,
    // Field 27: Historical summaries
    pub historical_summaries: List<HistoricalSummary, 1024>,
    // Field 28: Deposit requests start index
    pub deposit_requests_start_index: u64,
    // Field 29: Deposit balance to consume
    pub deposit_balance_to_consume: u64,
    // Field 30: Exit balance to consume
    pub exit_balance_to_consume: u64,
    // Field 31: Earliest exit epoch
    pub earliest_exit_epoch: u64,
    // Field 32: Consolidation balance to consume
    pub consolidation_balance_to_consume: u64,
    // Field 33: Earliest consolidation epoch
    pub earliest_consolidation_epoch: u64,
    // Field 34: Pending deposits
    pub pending_deposits: List<PendingDeposit, 256>,
    // Field 35: Pending partial withdrawals
    pub pending_partial_withdrawals: List<PendingPartialWithdrawal, 256>,
    // Field 36: Pending consolidations (small limit: 2^6 = 64)
    pub pending_consolidations: List<PendingConsolidation, 64>,
}

impl Default for MinimalBeaconState {
    fn default() -> Self {
        Self {
            genesis_time: 0,
            genesis_validators_root: [0u8; 32],
            slot: 0,
            fork: Fork::default(),
            latest_block_header: BeaconBlockHeader::default(),
            block_roots: Default::default(),
            state_roots: Default::default(),
            historical_roots: Default::default(),
            eth1_data: Eth1Data::default(),
            eth1_data_votes: Default::default(),
            eth1_deposit_index: 0,
            validators: Default::default(),
            balances: Default::default(),
            randao_mixes: Default::default(),
            slashings: Default::default(),
            previous_epoch_participation: Default::default(),
            current_epoch_participation: Default::default(),
            justification_bits: Default::default(),
            previous_justified_checkpoint: Checkpoint::default(),
            current_justified_checkpoint: Checkpoint::default(),
            finalized_checkpoint: Checkpoint::default(),
            inactivity_scores: Default::default(),
            current_sync_committee: SyncCommittee::default(),
            next_sync_committee: SyncCommittee::default(),
            latest_execution_payload_header: ExecutionPayloadHeaderMinimal::default(),
            next_withdrawal_index: 0,
            next_withdrawal_validator_index: 0,
            historical_summaries: Default::default(),
            deposit_requests_start_index: 0,
            deposit_balance_to_consume: 0,
            exit_balance_to_consume: 0,
            earliest_exit_epoch: 0,
            consolidation_balance_to_consume: 0,
            earliest_consolidation_epoch: 0,
            pending_deposits: Default::default(),
            pending_partial_withdrawals: Default::default(),
            pending_consolidations: Default::default(),
        }
    }
}

impl MinimalBeaconState {
    /// Tree depth for validators list: log2(1024) = 10
    pub const VALIDATORS_TREE_DEPTH: u32 = 10;
    
    /// Tree depth for pending consolidations list: log2(64) = 6
    pub const PENDING_CONSOLIDATIONS_TREE_DEPTH: u32 = 6;
    
    /// Get proof depth for validator fields (from state root)
    /// Path: state -> validators -> [i] -> field
    /// Depth: 6 (state) + 1 (list data root) + 10 (validators tree) + 3 (validator fields) = 20
    pub const VALIDATOR_PROOF_DEPTH_FROM_STATE: u32 = 6 + 1 + 10 + 3;
    
    /// Get proof depth for consolidation fields (from state root)
    /// Path: state -> pending_consolidations -> [i] -> field
    /// Depth: 6 (state) + 1 (list data root) + 6 (consolidations tree) + 1 (consolidation fields) = 14
    pub const CONSOLIDATION_PROOF_DEPTH_FROM_STATE: u32 = 6 + 1 + 6 + 1;
}

// ============================================================================
// Test-only BeaconState with tiny limits (for unit tests)
// ============================================================================

/// Tiny test BeaconState with very small list limits for unit testing.
/// This avoids the huge memory allocations needed for full-size proofs.
/// Note: This has 37 fields like the real Electra state for correct proof depths.
#[derive(Debug, Clone, PartialEq, Eq, SimpleSerialize)]
pub struct TestBeaconState {
    // Field 0: genesis_time
    pub genesis_time: u64,
    // Field 1: genesis_validators_root
    pub genesis_validators_root: [u8; 32],
    // Field 2: slot
    pub slot: u64,
    // Field 3: fork
    pub fork: Fork,
    // Field 4: latest_block_header
    pub latest_block_header: BeaconBlockHeader,
    // Field 5: block_roots (small for testing)
    pub block_roots: Vector<[u8; 32], 8>,
    // Field 6: state_roots
    pub state_roots: Vector<[u8; 32], 8>,
    // Field 7: historical_roots
    pub historical_roots: List<[u8; 32], 16>,
    // Field 8: eth1_data
    pub eth1_data: Eth1Data,
    // Field 9: eth1_data_votes
    pub eth1_data_votes: List<Eth1Data, 4>,
    // Field 10: eth1_deposit_index
    pub eth1_deposit_index: u64,
    // Field 11: validators (small limit for testing)
    pub validators: List<Validator, 64>,
    // Field 12: balances
    pub balances: List<u64, 64>,
    // Field 13: randao_mixes
    pub randao_mixes: Vector<[u8; 32], 8>,
    // Field 14: slashings
    pub slashings: Vector<u64, 8>,
    // Field 15: previous_epoch_participation
    pub previous_epoch_participation: List<u8, 64>,
    // Field 16: current_epoch_participation
    pub current_epoch_participation: List<u8, 64>,
    // Field 17: justification_bits
    pub justification_bits: Bitvector<4>,
    // Field 18: previous_justified_checkpoint
    pub previous_justified_checkpoint: Checkpoint,
    // Field 19: current_justified_checkpoint
    pub current_justified_checkpoint: Checkpoint,
    // Field 20: finalized_checkpoint
    pub finalized_checkpoint: Checkpoint,
    // Field 21: inactivity_scores
    pub inactivity_scores: List<u64, 64>,
    // Field 22: current_sync_committee (simplified as root)
    pub current_sync_committee_root: [u8; 32],
    // Field 23: next_sync_committee (simplified as root)
    pub next_sync_committee_root: [u8; 32],
    // Field 24: latest_execution_payload_header (simplified as root)
    pub latest_execution_payload_header_root: [u8; 32],
    // Field 25: next_withdrawal_index
    pub next_withdrawal_index: u64,
    // Field 26: next_withdrawal_validator_index
    pub next_withdrawal_validator_index: u64,
    // Field 27: historical_summaries
    pub historical_summaries: List<HistoricalSummary, 16>,
    // Field 28: deposit_requests_start_index
    pub deposit_requests_start_index: u64,
    // Field 29: deposit_balance_to_consume
    pub deposit_balance_to_consume: u64,
    // Field 30: exit_balance_to_consume
    pub exit_balance_to_consume: u64,
    // Field 31: earliest_exit_epoch
    pub earliest_exit_epoch: u64,
    // Field 32: consolidation_balance_to_consume
    pub consolidation_balance_to_consume: u64,
    // Field 33: earliest_consolidation_epoch
    pub earliest_consolidation_epoch: u64,
    // Field 34: pending_deposits (simplified)
    pub pending_deposits: List<[u8; 32], 16>,
    // Field 35: pending_partial_withdrawals
    pub pending_partial_withdrawals: List<PendingPartialWithdrawal, 16>,
    // Field 36: pending_consolidations (small limit for testing)
    pub pending_consolidations: List<PendingConsolidation, 8>,
}

impl Default for TestBeaconState {
    fn default() -> Self {
        Self {
            genesis_time: 0,
            genesis_validators_root: [0u8; 32],
            slot: 0,
            fork: Fork::default(),
            latest_block_header: BeaconBlockHeader::default(),
            block_roots: Default::default(),
            state_roots: Default::default(),
            historical_roots: Default::default(),
            eth1_data: Eth1Data::default(),
            eth1_data_votes: Default::default(),
            eth1_deposit_index: 0,
            validators: Default::default(),
            balances: Default::default(),
            randao_mixes: Default::default(),
            slashings: Default::default(),
            previous_epoch_participation: Default::default(),
            current_epoch_participation: Default::default(),
            justification_bits: Default::default(),
            previous_justified_checkpoint: Checkpoint::default(),
            current_justified_checkpoint: Checkpoint::default(),
            finalized_checkpoint: Checkpoint::default(),
            inactivity_scores: Default::default(),
            current_sync_committee_root: [0u8; 32],
            next_sync_committee_root: [0u8; 32],
            latest_execution_payload_header_root: [0u8; 32],
            next_withdrawal_index: 0,
            next_withdrawal_validator_index: 0,
            historical_summaries: Default::default(),
            deposit_requests_start_index: 0,
            deposit_balance_to_consume: 0,
            exit_balance_to_consume: 0,
            earliest_exit_epoch: 0,
            consolidation_balance_to_consume: 0,
            earliest_consolidation_epoch: 0,
            pending_deposits: Default::default(),
            pending_partial_withdrawals: Default::default(),
            pending_consolidations: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_hash_tree_root() {
        let mut validator = Validator::default();
        validator.effective_balance = 32_000_000_000;
        validator.activation_epoch = 100;
        validator.withdrawal_credentials[0] = 0x01;
        
        let root = validator.hash_tree_root().expect("should hash");
        let root_bytes: [u8; 32] = root.into();
        assert_ne!(root_bytes, [0u8; 32]);
    }

    #[test]
    fn test_pending_consolidation_hash_tree_root() {
        let consolidation = PendingConsolidation {
            source_index: 42,
            target_index: 100,
        };
        
        let root = consolidation.hash_tree_root().expect("should hash");
        let root_bytes: [u8; 32] = root.into();
        assert_ne!(root_bytes, [0u8; 32]);
    }

    #[test]
    fn test_minimal_beacon_state_hash_tree_root() {
        let state = MinimalBeaconState::default();
        let root = state.hash_tree_root().expect("should hash");
        let root_bytes: [u8; 32] = root.into();
        assert_ne!(root_bytes, [0u8; 32]);
    }
    
    #[test]
    fn test_validator_proof() {
        let mut validator = Validator::default();
        validator.withdrawal_credentials[0] = 0x01;
        validator.withdrawal_credentials[12..32].copy_from_slice(&[0xab; 20]);
        validator.activation_epoch = 100;
        
        // Prove withdrawal_credentials field
        let path: &[PathElement] = &["withdrawal_credentials".into()];
        let (proof, witness) = validator.prove(path).expect("should prove");
        
        // Verify the proof
        proof.verify(witness).expect("proof should be valid");
        
        // Check proof structure
        // Validator has 8 fields -> tree depth 3 (2^3 = 8)
        // withdrawal_credentials is field index 1 -> gindex = 8 + 1 = 9
        assert_eq!(proof.index, 9);
    }

    #[test]
    fn test_validator_activation_epoch_proof() {
        let mut validator = Validator::default();
        validator.activation_epoch = 12345;
        
        // Prove activation_epoch field
        let path: &[PathElement] = &["activation_epoch".into()];
        let (proof, witness) = validator.prove(path).expect("should prove");
        
        // Verify the proof
        proof.verify(witness).expect("proof should be valid");
        
        // Check proof structure (activation_epoch is field index 5 -> gindex 8+5=13)
        assert_eq!(proof.index, 13);
    }

    #[test]
    fn test_state_with_validators_proof() {
        // Create a small state with a few validators
        let mut state = MinimalBeaconState::default();
        
        for i in 0..5u8 {
            let mut validator = Validator::default();
            validator.withdrawal_credentials[0] = 0x01;
            validator.withdrawal_credentials[31] = i;
            validator.activation_epoch = 100 + i as u64;
            state.validators.push(validator);
            state.balances.push(32_000_000_000);
        }
        
        // Generate a proof for validators[2].withdrawal_credentials
        let path: &[PathElement] = &[
            "validators".into(),
            2usize.into(),
            "withdrawal_credentials".into(),
        ];
        
        let (proof, witness) = state.prove(path).expect("should prove");
        proof.verify(witness).expect("proof should be valid");
        
        // The proof should have branches
        assert!(!proof.branch.is_empty());
    }

    #[test]
    fn test_state_with_consolidations_proof() {
        let mut state = MinimalBeaconState::default();
        
        // Add validators
        for i in 0..3u8 {
            let mut validator = Validator::default();
            validator.withdrawal_credentials[0] = 0x01;
            validator.activation_epoch = 50 + i as u64;
            state.validators.push(validator);
            state.balances.push(32_000_000_000);
        }
        
        // Add a consolidation
        state.pending_consolidations.push(PendingConsolidation {
            source_index: 1,
            target_index: 0,
        });
        
        // Generate a proof for pending_consolidations[0].source_index
        let path: &[PathElement] = &[
            "pending_consolidations".into(),
            0usize.into(),
            "source_index".into(),
        ];
        
        let (proof, witness) = state.prove(path).expect("should prove");
        proof.verify(witness).expect("proof should be valid");
        
        assert!(!proof.branch.is_empty());
    }
}
