//! # Proof Generation Library
//!
//! Core proof generation logic for Gnosis consolidation incentives.
//! Generates SSZ Merkle proofs for:
//! - `pending_consolidations[i].source_index`
//! - `validators[i].withdrawal_credentials`
//! - `validators[i].activation_epoch`
//!
//! ## Architecture
//!
//! Two proof generation approaches are provided:
//!
//! 1. **`proof.rs` + `beacon_state.rs`**: Uses ssz_rs's built-in `Prove` trait
//!    directly on `MinimalBeaconState`/`TestBeaconState` with small list limits.
//!    Good for testing with small states.
//!
//! 2. **`sparse_proof.rs` + `state_prover.rs`**: Manual sparse Merkle proof
//!    generation that works with any list limits (including gnosis's 2^40 validators)
//!    without allocating full Merkle trees. Required for production use.

pub mod beacon_client;
pub mod beacon_state;
pub mod gindex;
pub mod proof;
pub mod sparse_proof;
pub mod state_prover;
pub mod types;

pub use beacon_client::BeaconClient;
pub use beacon_state::{MinimalBeaconState, BeaconBlockHeader as FullBeaconBlockHeader};
pub use gindex::GindexCalculator;
pub use proof::{ConsolidationProofBundle, ProofGenerator, ProofError};
pub use state_prover::StateProver;
pub use types::*;
