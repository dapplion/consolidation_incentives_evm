//! Generalized Index Computation
//!
//! Computes generalized indices (gindices) for SSZ Merkle proofs.
//! These must match the Solidity contract's hardcoded gindex functions.

use crate::types::preset;

/// Calculator for generalized indices in the beacon state tree
#[derive(Debug, Clone, Copy)]
pub struct GindexCalculator;

impl GindexCalculator {
    // BeaconState structure constants
    // BeaconState has 37 fields in Electra, giving a tree depth of 6 (2^6 = 64 >= 37)
    const BEACON_STATE_TREE_DEPTH: u32 = 6;
    const BEACON_STATE_BASE_GINDEX: u64 = 64; // 2^6

    // Field indices in BeaconState (0-indexed)
    const VALIDATORS_FIELD_INDEX: u64 = 11;
    const PENDING_CONSOLIDATIONS_FIELD_INDEX: u64 = 36;

    // BeaconBlockHeader structure constants
    // Header has 5 fields, tree depth 3 (2^3 = 8 >= 5)
    const HEADER_TREE_DEPTH: u32 = 3;
    const HEADER_BASE_GINDEX: u64 = 8; // 2^3

    // state_root is field index 3 in header
    const STATE_ROOT_FIELD_INDEX: u64 = 3;

    // Validator structure constants
    // Validator has 8 fields, tree depth 3 (2^3 = 8)
    const VALIDATOR_TREE_DEPTH: u32 = 3;
    const VALIDATOR_BASE_GINDEX: u64 = 8; // 2^3

    // Field indices in Validator
    const WITHDRAWAL_CREDENTIALS_FIELD_INDEX: u64 = 1;
    const ACTIVATION_EPOCH_FIELD_INDEX: u64 = 5;

    // PendingConsolidation has 2 fields, tree depth 1 (2^1 = 2)
    const CONSOLIDATION_TREE_DEPTH: u32 = 1;
    const CONSOLIDATION_BASE_GINDEX: u64 = 2; // 2^1

    // source_index is field index 0
    const SOURCE_INDEX_FIELD_INDEX: u64 = 0;

    /// Compute gindex for `pending_consolidations[i].source_index` from block root
    ///
    /// Path: header → state_root → pending_consolidations → [i] → source_index
    #[must_use]
    pub fn consolidation_source_gindex(consolidation_index: u64) -> u64 {
        // Start from header root
        // gindex(state_root in header) = 8 + 3 = 11
        let state_root_in_header = Self::HEADER_BASE_GINDEX + Self::STATE_ROOT_FIELD_INDEX;

        // gindex(pending_consolidations in state) = 64 + 36 = 100
        let pending_consolidations_in_state =
            Self::BEACON_STATE_BASE_GINDEX + Self::PENDING_CONSOLIDATIONS_FIELD_INDEX;

        // List data root is at gindex 2 * parent (left child for length, right child skipped, data at 2)
        // Actually for List, the tree is: [length_mix_in | data_root]
        // data_root is at gindex 2 relative to the list root (index 1 in 0-indexed, but gindex 2)
        // Wait, let me reconsider...
        // For a List<T, N>, the root is hash(data_root, length_mix_in)
        // - gindex 2: data_root (left child)
        // - gindex 3: length_mix_in (right child)

        // Depth of pending_consolidations list data tree
        let consolidations_data_depth = Self::pending_consolidations_tree_depth();

        // Element [i] in the data tree
        let element_gindex_in_data = (1_u64 << consolidations_data_depth) + consolidation_index;

        // source_index field in PendingConsolidation
        let source_in_consolidation =
            Self::CONSOLIDATION_BASE_GINDEX + Self::SOURCE_INDEX_FIELD_INDEX;

        // Combine paths:
        // block_root -> state_root: depth 3, gindex 11
        // state_root -> pending_consolidations: depth 6, gindex 100
        // pending_consolidations -> data_root: depth 1, gindex 2
        // data_root -> element[i]: depth varies, gindex = 2^depth + i
        // element[i] -> source_index: depth 1, gindex 2

        Self::concat_gindices(&[
            state_root_in_header,
            pending_consolidations_in_state,
            2, // data_root of list
            element_gindex_in_data,
            source_in_consolidation,
        ])
    }

    /// Compute gindex for `validators[i].withdrawal_credentials` from block root
    #[must_use]
    pub fn validator_credentials_gindex(validator_index: u64) -> u64 {
        let state_root_in_header = Self::HEADER_BASE_GINDEX + Self::STATE_ROOT_FIELD_INDEX;
        let validators_in_state = Self::BEACON_STATE_BASE_GINDEX + Self::VALIDATORS_FIELD_INDEX;
        let validators_data_depth = Self::validators_tree_depth();
        let element_gindex_in_data = (1_u64 << validators_data_depth) + validator_index;
        let credentials_in_validator =
            Self::VALIDATOR_BASE_GINDEX + Self::WITHDRAWAL_CREDENTIALS_FIELD_INDEX;

        Self::concat_gindices(&[
            state_root_in_header,
            validators_in_state,
            2, // data_root of list
            element_gindex_in_data,
            credentials_in_validator,
        ])
    }

    /// Compute gindex for `validators[i].activation_epoch` from block root
    #[must_use]
    pub fn validator_activation_epoch_gindex(validator_index: u64) -> u64 {
        let state_root_in_header = Self::HEADER_BASE_GINDEX + Self::STATE_ROOT_FIELD_INDEX;
        let validators_in_state = Self::BEACON_STATE_BASE_GINDEX + Self::VALIDATORS_FIELD_INDEX;
        let validators_data_depth = Self::validators_tree_depth();
        let element_gindex_in_data = (1_u64 << validators_data_depth) + validator_index;
        let activation_in_validator =
            Self::VALIDATOR_BASE_GINDEX + Self::ACTIVATION_EPOCH_FIELD_INDEX;

        Self::concat_gindices(&[
            state_root_in_header,
            validators_in_state,
            2, // data_root of list
            element_gindex_in_data,
            activation_in_validator,
        ])
    }

    /// Get the depth of the validators list data tree
    #[must_use]
    pub const fn validators_tree_depth() -> u32 {
        // VALIDATOR_REGISTRY_LIMIT = 2^40
        40
    }

    /// Get the depth of the pending_consolidations list data tree
    #[must_use]
    pub const fn pending_consolidations_tree_depth() -> u32 {
        preset::PENDING_CONSOLIDATIONS_DEPTH
    }

    /// Concatenate generalized indices along a path
    ///
    /// Given a sequence of gindices representing a path through nested structures,
    /// compute the final gindex from the outermost root.
    #[must_use]
    pub fn concat_gindices(gindices: &[u64]) -> u64 {
        let mut result = 1_u64; // Start at root

        for &gindex in gindices {
            let depth = 63 - gindex.leading_zeros(); // floor(log2(gindex))
            result = (result << depth) | (gindex ^ (1_u64 << depth));
        }

        result
    }

    /// Compute the depth (number of proof elements) for a given gindex
    #[must_use]
    pub const fn gindex_depth(gindex: u64) -> u32 {
        63 - gindex.leading_zeros()
    }

    /// Expected proof length for consolidation source_index
    #[must_use]
    pub fn consolidation_proof_length() -> u32 {
        let gindex = Self::consolidation_source_gindex(0);
        Self::gindex_depth(gindex)
    }

    /// Expected proof length for validator fields
    #[must_use]
    pub fn validator_proof_length() -> u32 {
        let gindex = Self::validator_credentials_gindex(0);
        Self::gindex_depth(gindex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concat_gindices_simple() {
        // Single gindex returns itself
        assert_eq!(GindexCalculator::concat_gindices(&[11]), 11);
    }

    #[test]
    fn test_concat_gindices_depth1() {
        // Root (1) -> left child (2)
        // concat([2]) should give 2
        assert_eq!(GindexCalculator::concat_gindices(&[2]), 2);
        // concat([3]) should give 3
        assert_eq!(GindexCalculator::concat_gindices(&[3]), 3);
    }

    #[test]
    fn test_concat_gindices_two_levels() {
        // Going to gindex 2, then to its left child (gindex 2 relative to that subtree)
        // Should give gindex 4 (2*2)
        assert_eq!(GindexCalculator::concat_gindices(&[2, 2]), 4);
        // Going to gindex 2, then to its right child (gindex 3)
        // Should give gindex 5 (2*2 + 1)
        assert_eq!(GindexCalculator::concat_gindices(&[2, 3]), 5);
    }

    #[test]
    fn test_gindex_depth() {
        assert_eq!(GindexCalculator::gindex_depth(1), 0); // root
        assert_eq!(GindexCalculator::gindex_depth(2), 1);
        assert_eq!(GindexCalculator::gindex_depth(3), 1);
        assert_eq!(GindexCalculator::gindex_depth(4), 2);
        assert_eq!(GindexCalculator::gindex_depth(7), 2);
        assert_eq!(GindexCalculator::gindex_depth(8), 3);
    }

    #[test]
    #[cfg(all(feature = "gnosis", not(feature = "minimal")))]
    fn test_consolidation_proof_length_gnosis() {
        // Expected: 3 (header) + 6 (state) + 1 (list) + 18 (data) + 1 (field) = 29
        assert_eq!(GindexCalculator::consolidation_proof_length(), 29);
    }

    #[test]
    #[cfg(all(feature = "gnosis", not(feature = "minimal")))]
    fn test_validator_proof_length_gnosis() {
        // Expected: 3 (header) + 6 (state) + 1 (list) + 40 (data) + 3 (field) = 53
        assert_eq!(GindexCalculator::validator_proof_length(), 53);
    }

    #[test]
    #[cfg(feature = "minimal")]
    fn test_consolidation_proof_length_minimal() {
        // Expected: 3 (header) + 6 (state) + 1 (list) + 6 (data) + 1 (field) = 17
        assert_eq!(GindexCalculator::consolidation_proof_length(), 17);
    }
}
