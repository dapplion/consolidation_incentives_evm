//! Beacon State SSZ Types
//!
//! Defines SSZ-compatible types matching the Electra BeaconState layout.
//! These types derive `ssz_rs` traits for serialization and Merkle proof generation.

use ssz_rs::prelude::*;

/// Preset constants - only one feature should be active
#[cfg(all(feature = "gnosis", not(feature = "minimal")))]
pub mod preset {
    /// Maximum number of validators (2^40)
    pub const VALIDATOR_REGISTRY_LIMIT: usize = 1_099_511_627_776;
    /// Maximum pending consolidations (2^18)
    pub const PENDING_CONSOLIDATIONS_LIMIT: usize = 262_144;
    /// Slots per epoch on Gnosis
    pub const SLOTS_PER_EPOCH: u64 = 16;
    /// Seconds per slot on Gnosis
    pub const SECONDS_PER_SLOT: u64 = 5;
    /// Pending consolidations tree depth
    pub const PENDING_CONSOLIDATIONS_DEPTH: u32 = 18;
}

#[cfg(feature = "minimal")]
pub mod preset {
    /// Validator registry limit for minimal preset
    pub const VALIDATOR_REGISTRY_LIMIT: usize = 1_099_511_627_776;
    /// Pending consolidations limit for minimal preset (2^6 = 64)
    pub const PENDING_CONSOLIDATIONS_LIMIT: usize = 64;
    /// Slots per epoch in minimal preset
    pub const SLOTS_PER_EPOCH: u64 = 8;
    /// Seconds per slot (same as mainnet for simplicity)
    pub const SECONDS_PER_SLOT: u64 = 6;
    /// Pending consolidations tree depth
    pub const PENDING_CONSOLIDATIONS_DEPTH: u32 = 6;
}

#[cfg(not(any(feature = "gnosis", feature = "minimal")))]
pub mod preset {
    /// Maximum number of validators (2^40)
    pub const VALIDATOR_REGISTRY_LIMIT: usize = 1_099_511_627_776;
    /// Maximum pending consolidations (2^18)
    pub const PENDING_CONSOLIDATIONS_LIMIT: usize = 262_144;
    /// Slots per epoch on Gnosis
    pub const SLOTS_PER_EPOCH: u64 = 16;
    /// Seconds per slot on Gnosis
    pub const SECONDS_PER_SLOT: u64 = 5;
    /// Pending consolidations tree depth
    pub const PENDING_CONSOLIDATIONS_DEPTH: u32 = 18;
}

/// Pending consolidation entry from the beacon state
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct PendingConsolidation {
    /// Source validator index being consolidated
    pub source_index: u64,
    /// Target validator index receiving the consolidation
    pub target_index: u64,
}

/// Serde-compatible version of PendingConsolidation for JSON serialization
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingConsolidationJson {
    /// Source validator index being consolidated
    pub source_index: u64,
    /// Target validator index receiving the consolidation
    pub target_index: u64,
}

impl From<PendingConsolidation> for PendingConsolidationJson {
    fn from(p: PendingConsolidation) -> Self {
        Self {
            source_index: p.source_index,
            target_index: p.target_index,
        }
    }
}

/// Validator record from the beacon state
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct Validator {
    /// BLS public key (48 bytes)
    pub pubkey: Vector<u8, 48>,
    /// Withdrawal credentials (32 bytes)
    pub withdrawal_credentials: [u8; 32],
    /// Effective balance in Gwei
    pub effective_balance: u64,
    /// Whether the validator is slashed
    pub slashed: bool,
    /// Epoch when validator became eligible for activation
    pub activation_eligibility_epoch: u64,
    /// Epoch when validator was activated
    pub activation_epoch: u64,
    /// Epoch when validator will exit
    pub exit_epoch: u64,
    /// Epoch when validator can withdraw
    pub withdrawable_epoch: u64,
}

/// Beacon block header
#[derive(Debug, Clone, Default, PartialEq, Eq, SimpleSerialize)]
pub struct BeaconBlockHeader {
    /// Slot number
    pub slot: u64,
    /// Proposer validator index
    pub proposer_index: u64,
    /// Root of the parent block
    pub parent_root: [u8; 32],
    /// Root of the beacon state
    pub state_root: [u8; 32],
    /// Root of the block body
    pub body_root: [u8; 32],
}

/// Checkpoint for finality (JSON-serializable, not SSZ)
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FinalityCheckpoints {
    /// Previous justified checkpoint epoch
    pub previous_justified_epoch: u64,
    /// Current justified checkpoint epoch
    pub current_justified_epoch: u64,
    /// Finalized checkpoint epoch
    pub finalized_epoch: u64,
    /// Finalized checkpoint root
    #[serde(with = "hex_bytes32")]
    pub finalized_root: [u8; 32],
}

// Hex encoding helpers for serde
mod hex_bytes32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("0x{}", hex::encode(bytes)))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let s = s.strip_prefix("0x").unwrap_or(&s);
        let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_consolidation_ssz_roundtrip() {
        let consolidation = PendingConsolidation {
            source_index: 42,
            target_index: 100,
        };

        let encoded = ssz_rs::serialize(&consolidation).expect("serialize");
        let decoded: PendingConsolidation =
            ssz_rs::deserialize(&encoded).expect("deserialize");

        assert_eq!(consolidation, decoded);
    }

    #[test]
    fn test_validator_ssz_roundtrip() {
        let mut validator = Validator::default();
        validator.effective_balance = 32_000_000_000;
        validator.activation_epoch = 100;
        validator.withdrawal_credentials[0] = 0x01;

        let encoded = ssz_rs::serialize(&validator).expect("serialize");
        let decoded: Validator = ssz_rs::deserialize(&encoded).expect("deserialize");

        assert_eq!(validator, decoded);
    }

    #[test]
    fn test_beacon_block_header_ssz_roundtrip() {
        let header = BeaconBlockHeader {
            slot: 12345,
            proposer_index: 42,
            parent_root: [1u8; 32],
            state_root: [2u8; 32],
            body_root: [3u8; 32],
        };

        let encoded = ssz_rs::serialize(&header).expect("serialize");
        let decoded: BeaconBlockHeader = ssz_rs::deserialize(&encoded).expect("deserialize");

        assert_eq!(header, decoded);
    }
}
