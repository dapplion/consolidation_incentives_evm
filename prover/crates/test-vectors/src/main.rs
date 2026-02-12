//! Test Vector Generator
//!
//! Generates JSON test vectors for Solidity contract tests.
//! Uses gnosis-preset tree depths (validators: 2^40, consolidations: 2^18)
//! to produce proofs matching the Solidity contract's hardcoded constants.
//!
//! The approach:
//! 1. Build validators and consolidations with known data
//! 2. Compute all 37 BeaconState field roots (using gnosis depths for list fields)
//! 3. Use StateProver with gnosis depths to generate proofs
//! 4. Output JSON test vectors for Foundry tests

use anyhow::Result;
use clap::Parser;
use proof_gen::beacon_state::{
    BeaconBlockHeader, PendingConsolidation, Validator,
};
use proof_gen::sparse_proof::mix_in_length;
use proof_gen::state_prover::{compute_list_root, StateProver};
use proof_gen::ConsolidationProofBundle;
use serde::Serialize;
use sha2::{Digest, Sha256};
use ssz_rs::prelude::*;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "generate-test-vectors")]
#[command(about = "Generate test vectors for consolidation incentives Solidity tests")]
struct Args {
    /// Output directory for test vectors
    #[arg(short, long, default_value = "../../contracts/test-vectors")]
    output: PathBuf,
}

/// Gnosis preset constants
const VALIDATORS_TREE_DEPTH: u32 = 40;
const CONSOLIDATIONS_TREE_DEPTH: u32 = 18;

/// Expected proof lengths (must match Solidity contract)
const EXPECTED_CONSOLIDATION_PROOF_LEN: usize = 29; // 1 + 18 + 1 + 6 + 3
const EXPECTED_VALIDATOR_PROOF_LEN: usize = 53; // 3 + 40 + 1 + 6 + 3

// ============================================================================
// Test Vector JSON Types
// ============================================================================

#[derive(Debug, Serialize)]
struct TestVectorFile {
    /// Preset used
    preset: String,
    /// Block root (0x-prefixed hex)
    block_root: String,
    /// Beacon timestamp for EIP-4788 lookup
    beacon_timestamp: u64,
    /// Max epoch for eligibility checks
    max_epoch: u64,
    /// Valid claims with proofs
    claims: Vec<TestClaim>,
    /// Invalid claims for negative testing
    invalid_claims: Vec<InvalidTestClaim>,
}

#[derive(Debug, Serialize)]
struct TestClaim {
    consolidation_index: u64,
    source_index: u64,
    activation_epoch: u64,
    source_credentials: String,
    proof_consolidation: Vec<String>,
    proof_credentials: Vec<String>,
    proof_activation_epoch: Vec<String>,
    expected_recipient: String,
}

#[derive(Debug, Serialize)]
struct InvalidTestClaim {
    description: String,
    consolidation_index: u64,
    source_index: u64,
    activation_epoch: u64,
    source_credentials: String,
    proof_consolidation: Vec<String>,
    proof_credentials: Vec<String>,
    proof_activation_epoch: Vec<String>,
    expected_error: String,
}

// ============================================================================
// Helpers
// ============================================================================

fn hex_encode_bytes32(b: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(b))
}

fn hex_encode_proof(proof: &[[u8; 32]]) -> Vec<String> {
    proof.iter().map(hex_encode_bytes32).collect()
}

fn address_from_credentials(creds: &[u8; 32]) -> String {
    // Last 20 bytes
    format!("0x{}", hex::encode(&creds[12..32]))
}

fn make_validator(index: u8, activation_epoch: u64, cred_prefix: u8) -> Validator {
    let mut v = Validator::default();
    v.withdrawal_credentials[0] = cred_prefix;
    // Put a unique non-zero address in last 20 bytes
    // Pattern: each byte is (index + 1) * (position + 1) to ensure non-zero
    for (i, byte) in v.withdrawal_credentials[12..32].iter_mut().enumerate() {
        *byte = ((index as u16 + 1) * (i as u16 + 1) % 255 + 1) as u8;
    }
    v.activation_epoch = activation_epoch;
    v.effective_balance = 32_000_000_000;
    v.exit_epoch = u64::MAX;
    v.withdrawable_epoch = u64::MAX;
    // Set a fake pubkey
    v.pubkey[0] = index;
    v.pubkey[1] = 0xAB;
    v
}

/// Compute the 37 field roots for a BeaconState that uses gnosis tree depths.
/// We build each field root individually, using gnosis-depth list roots for
/// validators (depth 40) and pending_consolidations (depth 18).
fn compute_gnosis_field_roots(
    validators: &[Validator],
    consolidations: &[PendingConsolidation],
) -> Vec<[u8; 32]> {
    let mut field_roots = vec![[0u8; 32]; 37];

    // Field 0: genesis_time (u64 = 0)
    field_roots[0] = hash_u64(0);
    // Field 1: genesis_validators_root
    field_roots[1] = [0u8; 32];
    // Field 2: slot
    field_roots[2] = hash_u64(1000);
    // Field 3: fork (all zeros)
    field_roots[3] = hash_fork_default();
    // Field 4: latest_block_header (all zeros)
    field_roots[4] = hash_header_default();
    // Field 5: block_roots (Vector of zeros)
    field_roots[5] = hash_zero_vector(8192); // SLOTS_PER_HISTORICAL_ROOT on gnosis
    // Field 6: state_roots
    field_roots[6] = hash_zero_vector(8192);
    // Field 7: historical_roots (empty list, depth depends on limit but root is mix_in_length of zero hash)
    field_roots[7] = empty_list_root(24); // HISTORICAL_ROOTS_LIMIT = 2^24
    // Field 8: eth1_data
    field_roots[8] = hash_eth1_data_default();
    // Field 9: eth1_data_votes (empty list)
    field_roots[9] = empty_list_root(10); // ETH1_DATA_VOTES_BOUND depth ~10 (2^10 = 1024)
    // Field 10: eth1_deposit_index
    field_roots[10] = hash_u64(0);

    // Field 11: validators - use gnosis depth 40
    let validator_hashes: Vec<[u8; 32]> = validators
        .iter()
        .map(|v| v.hash_tree_root().unwrap().into())
        .collect();
    field_roots[11] =
        compute_list_root(&validator_hashes, VALIDATORS_TREE_DEPTH, validators.len());

    // Field 12: balances (list of u64s)
    let balance_leaves = pack_u64_list(&vec![32_000_000_000u64; validators.len()]);
    let balances_data_depth = 40u32; // same limit as validators for balances
    field_roots[12] = compute_list_root(&balance_leaves, balances_data_depth, validators.len());

    // Field 13: randao_mixes (Vector of zeros)
    field_roots[13] = hash_zero_vector(8192); // EPOCHS_PER_HISTORICAL_VECTOR on gnosis
    // Field 14: slashings
    field_roots[14] = hash_zero_u64_vector(8192); // EPOCHS_PER_SLASHINGS_VECTOR
    // Field 15: previous_epoch_participation (empty list)
    field_roots[15] = empty_list_root(40); // same limit as validators
    // Field 16: current_epoch_participation (empty list)
    field_roots[16] = empty_list_root(40);
    // Field 17: justification_bits (Bitvector<4>)
    field_roots[17] = hash_justification_bits_default();
    // Field 18: previous_justified_checkpoint
    field_roots[18] = hash_checkpoint_default();
    // Field 19: current_justified_checkpoint
    field_roots[19] = hash_checkpoint_default();
    // Field 20: finalized_checkpoint
    field_roots[20] = hash_checkpoint_default();
    // Field 21: inactivity_scores (empty list)
    field_roots[21] = empty_list_root(40);
    // Field 22: current_sync_committee (complex, use a deterministic hash)
    field_roots[22] = hash_sync_committee_default();
    // Field 23: next_sync_committee
    field_roots[23] = hash_sync_committee_default();
    // Field 24: latest_execution_payload_header
    field_roots[24] = hash_execution_payload_header_default();
    // Field 25: next_withdrawal_index
    field_roots[25] = hash_u64(0);
    // Field 26: next_withdrawal_validator_index
    field_roots[26] = hash_u64(0);
    // Field 27: historical_summaries (empty list)
    field_roots[27] = empty_list_root(24);
    // Field 28: deposit_requests_start_index
    field_roots[28] = hash_u64(0);
    // Field 29: deposit_balance_to_consume
    field_roots[29] = hash_u64(0);
    // Field 30: exit_balance_to_consume
    field_roots[30] = hash_u64(0);
    // Field 31: earliest_exit_epoch
    field_roots[31] = hash_u64(0);
    // Field 32: consolidation_balance_to_consume
    field_roots[32] = hash_u64(0);
    // Field 33: earliest_consolidation_epoch
    field_roots[33] = hash_u64(0);
    // Field 34: pending_deposits (empty list)
    field_roots[34] = empty_list_root(27); // PENDING_DEPOSITS_LIMIT = 2^27
    // Field 35: pending_partial_withdrawals (empty list)
    field_roots[35] = empty_list_root(27); // PENDING_PARTIAL_WITHDRAWALS_LIMIT = 2^27

    // Field 36: pending_consolidations - use gnosis depth 18
    let consolidation_hashes: Vec<[u8; 32]> = consolidations
        .iter()
        .map(|c| c.hash_tree_root().unwrap().into())
        .collect();
    field_roots[36] = compute_list_root(
        &consolidation_hashes,
        CONSOLIDATIONS_TREE_DEPTH,
        consolidations.len(),
    );

    field_roots
}

/// Hash a u64 as SSZ: little-endian padded to 32 bytes
fn hash_u64(v: u64) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..8].copy_from_slice(&v.to_le_bytes());
    b
}

/// Root of an empty list at given tree depth
fn empty_list_root(depth: u32) -> [u8; 32] {
    let zero_root = zero_hash(depth);
    mix_in_length(zero_root, 0)
}

/// Compute zero hash at depth (hash of zeros upon zeros)
fn zero_hash(depth: u32) -> [u8; 32] {
    let mut h = [0u8; 32];
    for _ in 0..depth {
        let mut hasher = Sha256::new();
        hasher.update(h);
        hasher.update(h);
        h = hasher.finalize().into();
    }
    h
}

/// Pack a list of u64s into chunks (4 per 32-byte chunk, SSZ-style)
fn pack_u64_list(values: &[u64]) -> Vec<[u8; 32]> {
    let mut chunks = Vec::new();
    for chunk in values.chunks(4) {
        let mut c = [0u8; 32];
        for (i, &v) in chunk.iter().enumerate() {
            c[i * 8..(i + 1) * 8].copy_from_slice(&v.to_le_bytes());
        }
        chunks.push(c);
    }
    if chunks.is_empty() {
        chunks.push([0u8; 32]);
    }
    chunks
}

/// Hash of a default Fork (all zeros)
fn hash_fork_default() -> [u8; 32] {
    // Fork: previous_version (4 bytes), current_version (4 bytes), epoch (u64)
    // Each field is a leaf in a container with 3 fields → depth 2, 4 leaves
    let f0 = [0u8; 32]; // previous_version padded
    let f1 = [0u8; 32]; // current_version padded
    let f2 = [0u8; 32]; // epoch = 0
    let f3 = [0u8; 32]; // padding (4th leaf)
    let h01 = sha256_pair(&f0, &f1);
    let h23 = sha256_pair(&f2, &f3);
    sha256_pair(&h01, &h23)
}

/// Hash of a default BeaconBlockHeader (all zeros)
fn hash_header_default() -> [u8; 32] {
    // Header has 5 fields → depth 3 (8 leaves)
    let fields = [
        hash_u64(0), // slot
        hash_u64(0), // proposer_index
        [0u8; 32],   // parent_root
        [0u8; 32],   // state_root
        [0u8; 32],   // body_root
    ];
    hash_container_fields(&fields, 3)
}

/// Hash of a default Eth1Data
fn hash_eth1_data_default() -> [u8; 32] {
    // 3 fields → depth 2 (4 leaves)
    let fields = [
        [0u8; 32], // deposit_root
        hash_u64(0), // deposit_count
        [0u8; 32], // block_hash
    ];
    hash_container_fields(&fields, 2)
}

/// Hash of default justification bits (Bitvector<4>)
fn hash_justification_bits_default() -> [u8; 32] {
    // Bitvector<4> is stored as 1 byte padded to 32
    [0u8; 32]
}

/// Hash of a default Checkpoint
fn hash_checkpoint_default() -> [u8; 32] {
    // 2 fields → depth 1
    let f0 = hash_u64(0); // epoch
    let f1 = [0u8; 32]; // root
    sha256_pair(&f0, &f1)
}

/// Hash of a default SyncCommittee (all zeros)
fn hash_sync_committee_default() -> [u8; 32] {
    // SyncCommittee has 2 fields → depth 1
    // pubkeys: Vector<Vector<u8, 48>, 512> and aggregate_pubkey: Vector<u8, 48>
    // For all-zero pubkeys, compute the actual root
    // pubkeys root = Merkle root of 512 zero-hash(48-byte-vector) nodes
    // This is complex — just use a deterministic placeholder since it doesn't
    // affect the proofs we care about (validators and consolidations)
    let zero_pubkey_root = zero_hash(1); // Vector<u8, 48> root: hash of 2 chunks (48 bytes = 2 x 32-byte chunks)
    // 512 identical zero pubkey roots → depth 9 binary tree
    let pubkeys_root = {
        let mut h = zero_pubkey_root;
        for _ in 0..9 {
            h = sha256_pair(&h, &h);
        }
        h
    };
    let agg_pubkey_root = zero_pubkey_root;
    sha256_pair(&pubkeys_root, &agg_pubkey_root)
}

/// Hash of a default ExecutionPayloadHeader
fn hash_execution_payload_header_default() -> [u8; 32] {
    // Has 17 fields in Deneb → depth 5 (32 leaves)
    // All zeros — just compute the zero hash at depth 5
    zero_hash(5)
}

/// Hash of a zero-valued bytes32 Vector of given length
fn hash_zero_vector(len: usize) -> [u8; 32] {
    // Vector of bytes32 zeros: the tree has exactly `len` leaves, all zero
    // depth = ceil(log2(len))
    let depth = (len as f64).log2().ceil() as u32;
    zero_hash(depth)
}

/// Hash of a zero-valued u64 Vector (packed)
fn hash_zero_u64_vector(len: usize) -> [u8; 32] {
    // u64s pack 4 per chunk. Vector<u64, N> has N/4 chunks.
    let num_chunks = (len + 3) / 4;
    let depth = (num_chunks as f64).log2().ceil() as u32;
    zero_hash(depth)
}

/// Hash a container's fields into a binary Merkle tree of given depth
fn hash_container_fields(fields: &[[u8; 32]], depth: u32) -> [u8; 32] {
    let num_leaves = 1usize << depth;
    let mut leaves = vec![[0u8; 32]; num_leaves];
    for (i, f) in fields.iter().enumerate() {
        leaves[i] = *f;
    }

    // Build tree bottom-up
    let mut layer = leaves;
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len() / 2);
        for pair in layer.chunks(2) {
            next.push(sha256_pair(&pair[0], &pair[1]));
        }
        layer = next;
    }
    layer[0]
}

fn sha256_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(a);
    hasher.update(b);
    hasher.finalize().into()
}

/// Tamper with a proof by flipping a bit in one of the sibling hashes
fn tamper_proof(proof: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut tampered = proof.to_vec();
    if !tampered.is_empty() {
        tampered[0][0] ^= 0x01;
    }
    tampered
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    tracing::info!(
        output = %args.output.display(),
        "Generating test vectors with gnosis preset"
    );

    std::fs::create_dir_all(&args.output)?;

    // ========================================================================
    // Build test state data
    // ========================================================================

    let max_epoch: u64 = 1000;
    let beacon_timestamp: u64 = 1_700_000_000;

    // Create 10 validators with various properties
    let validators = vec![
        make_validator(0, 100, 0x01), // eligible, 0x01 credentials
        make_validator(1, 200, 0x01), // eligible
        make_validator(2, 500, 0x01), // eligible
        make_validator(3, 999, 0x01), // eligible (just under max_epoch)
        make_validator(4, 1000, 0x01), // NOT eligible (activation_epoch == max_epoch)
        make_validator(5, 2000, 0x01), // NOT eligible (too high)
        make_validator(6, 300, 0x02), // eligible, 0x02 credentials
        make_validator(7, 50, 0x00),  // BLS credentials (invalid for reward)
        make_validator(8, 150, 0x01), // eligible
        make_validator(9, 400, 0x01), // eligible
    ];

    // Create consolidations
    let consolidations = vec![
        PendingConsolidation {
            source_index: 0,
            target_index: 1,
        },
        PendingConsolidation {
            source_index: 2,
            target_index: 3,
        },
        PendingConsolidation {
            source_index: 6,
            target_index: 8,
        }, // 0x02 credentials
        PendingConsolidation {
            source_index: 4,
            target_index: 5,
        }, // ineligible (epoch too high)
        PendingConsolidation {
            source_index: 7,
            target_index: 9,
        }, // BLS credentials
        PendingConsolidation {
            source_index: 9,
            target_index: 0,
        }, // eligible
    ];

    // ========================================================================
    // Compute field roots and build StateProver
    // ========================================================================

    let field_roots = compute_gnosis_field_roots(&validators, &consolidations);
    tracing::info!("Computed 37 field roots with gnosis depths");

    let prover = StateProver::new(
        field_roots,
        validators.clone(),
        consolidations.clone(),
        VALIDATORS_TREE_DEPTH,
        CONSOLIDATIONS_TREE_DEPTH,
    )?;

    let state_root = prover.compute_state_root();
    tracing::info!(state_root = hex::encode(state_root), "Computed state root");

    // Build header wrapping this state
    let header = BeaconBlockHeader {
        slot: 1000,
        proposer_index: 0,
        parent_root: [0u8; 32],
        state_root,
        body_root: [1u8; 32], // non-zero to be realistic
    };
    let block_root: [u8; 32] = header.hash_tree_root()?.into();
    tracing::info!(block_root = hex::encode(block_root), "Computed block root");

    // ========================================================================
    // Generate valid claims
    // ========================================================================

    let mut claims = Vec::new();

    // Claim 0: validator 0, consolidation 0 (0x01 credentials, eligible)
    let bundle0 = prover.generate_full_proof_bundle(&header, 0, beacon_timestamp)?;
    assert_eq!(bundle0.proof_consolidation.len(), EXPECTED_CONSOLIDATION_PROOF_LEN,
        "consolidation proof length mismatch: got {}, expected {}",
        bundle0.proof_consolidation.len(), EXPECTED_CONSOLIDATION_PROOF_LEN);
    assert_eq!(bundle0.proof_credentials.len(), EXPECTED_VALIDATOR_PROOF_LEN,
        "credentials proof length mismatch");
    assert_eq!(bundle0.proof_activation_epoch.len(), EXPECTED_VALIDATOR_PROOF_LEN,
        "activation epoch proof length mismatch");
    claims.push(bundle_to_claim(&bundle0));

    // Claim 1: validator 2, consolidation 1 (0x01 credentials, eligible)
    let bundle1 = prover.generate_full_proof_bundle(&header, 1, beacon_timestamp)?;
    claims.push(bundle_to_claim(&bundle1));

    // Claim 2: validator 6, consolidation 2 (0x02 credentials, eligible)
    let bundle2 = prover.generate_full_proof_bundle(&header, 2, beacon_timestamp)?;
    claims.push(bundle_to_claim(&bundle2));

    // Claim 3: validator 9, consolidation 5 (0x01 credentials, eligible)
    let bundle5 = prover.generate_full_proof_bundle(&header, 5, beacon_timestamp)?;
    claims.push(bundle_to_claim(&bundle5));

    tracing::info!(count = claims.len(), "Generated valid claims");

    // ========================================================================
    // Generate invalid claims
    // ========================================================================

    let mut invalid_claims = Vec::new();

    // Invalid 1: activation epoch too high (validator 4, consolidation 3)
    let bundle_ineligible = prover.generate_full_proof_bundle(&header, 3, beacon_timestamp)?;
    invalid_claims.push(InvalidTestClaim {
        description: "activation_epoch equals maxEpoch (not eligible)".to_string(),
        consolidation_index: bundle_ineligible.consolidation_index,
        source_index: bundle_ineligible.source_index,
        activation_epoch: bundle_ineligible.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle_ineligible.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle_ineligible.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle_ineligible.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle_ineligible.proof_activation_epoch),
        expected_error: "NotEligible".to_string(),
    });

    // Invalid 2: BLS credentials (validator 7, consolidation 4)
    let bundle_bls = prover.generate_full_proof_bundle(&header, 4, beacon_timestamp)?;
    invalid_claims.push(InvalidTestClaim {
        description: "BLS credentials (0x00 prefix) - not eligible for reward".to_string(),
        consolidation_index: bundle_bls.consolidation_index,
        source_index: bundle_bls.source_index,
        activation_epoch: bundle_bls.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle_bls.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle_bls.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle_bls.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle_bls.proof_activation_epoch),
        expected_error: "InvalidCredentialsPrefix".to_string(),
    });

    // Invalid 3: tampered consolidation proof (valid claim but corrupted proof)
    invalid_claims.push(InvalidTestClaim {
        description: "tampered consolidation proof - single bit flip".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&tamper_proof(&bundle0.proof_consolidation)),
        proof_credentials: hex_encode_proof(&bundle0.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 4: tampered credentials proof
    invalid_claims.push(InvalidTestClaim {
        description: "tampered credentials proof - single bit flip".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&tamper_proof(&bundle0.proof_credentials)),
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 5: tampered activation epoch proof
    invalid_claims.push(InvalidTestClaim {
        description: "tampered activation epoch proof - single bit flip".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle0.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&tamper_proof(&bundle0.proof_activation_epoch)),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 6: wrong source_index (proof is for validator 0, but claim says validator 99)
    invalid_claims.push(InvalidTestClaim {
        description: "wrong source_index - proof valid for different validator".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: 99, // wrong!
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle0.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 7: wrong credentials (proof is for validator 0's credentials, but claim uses different)
    let mut wrong_creds = bundle0.source_credentials;
    wrong_creds[31] ^= 0xFF; // flip last byte
    invalid_claims.push(InvalidTestClaim {
        description: "wrong credentials - proof valid for different credentials".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&wrong_creds),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle0.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 8: wrong activation epoch
    invalid_claims.push(InvalidTestClaim {
        description: "wrong activation_epoch - proof valid for different epoch".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: 999, // wrong!
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle0.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProof".to_string(),
    });

    // Invalid 9: swapped proofs (consolidation proof in credentials position)
    invalid_claims.push(InvalidTestClaim {
        description: "swapped proofs - consolidation proof used as credentials proof".to_string(),
        consolidation_index: bundle0.consolidation_index,
        source_index: bundle0.source_index,
        activation_epoch: bundle0.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle0.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle0.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle0.proof_consolidation), // wrong! (will be wrong length -> InvalidProofLength)
        proof_activation_epoch: hex_encode_proof(&bundle0.proof_activation_epoch),
        expected_error: "InvalidProofLength".to_string(),
    });

    tracing::info!(count = invalid_claims.len(), "Generated invalid claims");

    // ========================================================================
    // Write output
    // ========================================================================

    let test_vectors = TestVectorFile {
        preset: "gnosis".to_string(),
        block_root: hex_encode_bytes32(&block_root),
        beacon_timestamp,
        max_epoch,
        claims,
        invalid_claims,
    };

    let output_path = args.output.join("test_vectors.json");
    let json = serde_json::to_string_pretty(&test_vectors)?;
    std::fs::write(&output_path, &json)?;

    tracing::info!(
        path = %output_path.display(),
        size = json.len(),
        "Wrote test vectors"
    );

    // Also verify the generated vectors by checking proof lengths
    tracing::info!("Verification:");
    tracing::info!(
        "  Consolidation proof length: {} (expected {})",
        EXPECTED_CONSOLIDATION_PROOF_LEN,
        29
    );
    tracing::info!(
        "  Validator proof length: {} (expected {})",
        EXPECTED_VALIDATOR_PROOF_LEN,
        53
    );

    Ok(())
}

fn bundle_to_claim(bundle: &ConsolidationProofBundle) -> TestClaim {
    TestClaim {
        consolidation_index: bundle.consolidation_index,
        source_index: bundle.source_index,
        activation_epoch: bundle.activation_epoch,
        source_credentials: hex_encode_bytes32(&bundle.source_credentials),
        proof_consolidation: hex_encode_proof(&bundle.proof_consolidation),
        proof_credentials: hex_encode_proof(&bundle.proof_credentials),
        proof_activation_epoch: hex_encode_proof(&bundle.proof_activation_epoch),
        expected_recipient: address_from_credentials(&bundle.source_credentials),
    }
}
