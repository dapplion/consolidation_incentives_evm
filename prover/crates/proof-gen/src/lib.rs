//! # Proof Generation Library
//!
//! Core proof generation logic for Gnosis consolidation incentives.
//! Generates SSZ Merkle proofs for:
//! - `pending_consolidations[i].source_index`
//! - `validators[i].withdrawal_credentials`
//! - `validators[i].activation_epoch`

pub mod beacon_client;
pub mod beacon_state;
pub mod gindex;
pub mod proof;
pub mod sparse_proof;
pub mod types;

pub use beacon_client::BeaconClient;
pub use beacon_state::{MinimalBeaconState, BeaconBlockHeader as FullBeaconBlockHeader};
pub use gindex::GindexCalculator;
pub use proof::{ConsolidationProofBundle, ProofGenerator, ProofError};
pub use types::*;
