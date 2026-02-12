// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/**
 * @title SSZMerkleVerifier
 * @notice Library for verifying SSZ Merkle proofs using generalized indices
 * @dev Uses SHA256 precompile (address 0x02) for hashing
 * 
 * Generalized Index (gindex) encoding:
 * - Root has gindex 1
 * - For any node at gindex g: left child is 2*g, right child is 2*g+1
 * - The bits of gindex (excluding leading 1) encode the path from root to leaf
 *   - 0 bit = left, 1 bit = right
 * - The depth is floor(log2(gindex))
 * 
 * Gnosis Chain Electra BeaconState structure:
 * - BeaconState has 37 fields → tree depth 6 (generalizes to depth 64 = 2^6)
 * - validators field index: 11 → gindex in BeaconState: 64 + 11 = 75
 * - pending_consolidations field index: 36 → gindex in BeaconState: 64 + 36 = 100
 * - state_root in BeaconBlockHeader has field index 3 → gindex: 8 + 3 = 11
 */
library SSZMerkleVerifier {
    /// @notice SHA256 precompile address
    address private constant SHA256_PRECOMPILE = address(0x02);

    // =========================================================================
    // Gnosis Chain Constants (Electra fork)
    // =========================================================================
    
    /// @dev BeaconBlockHeader tree depth (8 fields → depth 3, base gindex 8)
    uint256 private constant HEADER_DEPTH = 3;
    /// @dev state_root field index in BeaconBlockHeader
    uint256 private constant STATE_ROOT_FIELD_INDEX = 3;
    /// @dev Gindex of state_root in BeaconBlockHeader (from block_root)
    uint256 private constant STATE_ROOT_GINDEX_IN_HEADER = 11; // 8 + 3
    
    /// @dev BeaconState tree depth (37 fields → depth 6, base gindex 64)
    uint256 private constant STATE_DEPTH = 6;
    /// @dev validators field index in BeaconState
    uint256 private constant VALIDATORS_FIELD_INDEX = 11;
    /// @dev pending_consolidations field index in BeaconState
    uint256 private constant PENDING_CONSOLIDATIONS_FIELD_INDEX = 36;
    /// @dev Gindex of validators in BeaconState
    uint256 private constant VALIDATORS_GINDEX_IN_STATE = 75; // 64 + 11
    /// @dev Gindex of pending_consolidations in BeaconState
    uint256 private constant PENDING_CONSOLIDATIONS_GINDEX_IN_STATE = 100; // 64 + 36
    
    /// @dev VALIDATOR_REGISTRY_LIMIT = 2^40, so list data tree depth is 40
    uint256 private constant VALIDATOR_LIST_DEPTH = 40;
    /// @dev PENDING_CONSOLIDATIONS_LIMIT = 2^18, so list data tree depth is 18
    uint256 private constant CONSOLIDATION_LIST_DEPTH = 18;
    
    /// @dev Validator container has 8 fields → depth 3
    uint256 private constant VALIDATOR_DEPTH = 3;
    /// @dev withdrawal_credentials field index in Validator
    uint256 private constant WITHDRAWAL_CREDENTIALS_FIELD_INDEX = 1;
    /// @dev activation_epoch field index in Validator
    uint256 private constant ACTIVATION_EPOCH_FIELD_INDEX = 5;
    
    /// @dev PendingConsolidation container has 2 fields → depth 1
    uint256 private constant CONSOLIDATION_DEPTH = 1;
    /// @dev source_index field index in PendingConsolidation
    uint256 private constant SOURCE_INDEX_FIELD_INDEX = 0;

    // =========================================================================
    // Expected proof lengths (from block_root to leaf)
    // =========================================================================
    
    /// @notice Expected proof length for pending_consolidations[i].source_index
    /// @dev header(3) + state(6) + list_mix_in(1) + list_data(18) + consolidation(1) = 29
    uint256 public constant CONSOLIDATION_PROOF_LENGTH = 29;
    
    /// @notice Expected proof length for validators[i].withdrawal_credentials or activation_epoch
    /// @dev header(3) + state(6) + list_mix_in(1) + list_data(40) + validator(3) = 53
    uint256 public constant VALIDATOR_PROOF_LENGTH = 53;

    // =========================================================================
    // Core Verification
    // =========================================================================

    /**
     * @notice Verifies an SSZ Merkle proof
     * @param root The Merkle root to verify against
     * @param leaf The leaf value being proven
     * @param proof Array of sibling hashes from leaf to root
     * @param gindex The generalized index of the leaf
     * @return True if the proof is valid
     */
    function verifyProof(
        bytes32 root,
        bytes32 leaf,
        bytes32[] calldata proof,
        uint256 gindex
    ) internal view returns (bool) {
        // Gindex must be at least 1 (the root)
        if (gindex == 0) {
            return false;
        }
        
        // Special case: gindex 1 means the leaf IS the root
        if (gindex == 1) {
            return proof.length == 0 && leaf == root;
        }
        
        // Proof length must match the depth (number of bits in gindex minus the leading 1)
        uint256 depth = _log2(gindex);
        if (proof.length != depth) {
            return false;
        }
        
        // Compute root from leaf and proof
        bytes32 computed = leaf;
        uint256 index = gindex;
        
        for (uint256 i = 0; i < proof.length; i++) {
            // If the current index is even, we're a left child; sibling is on right
            // If odd, we're a right child; sibling is on left
            if (index & 1 == 0) {
                // Even index: we're left child, sibling is right
                computed = _sha256Pair(computed, proof[i]);
            } else {
                // Odd index: we're right child, sibling is left
                computed = _sha256Pair(proof[i], computed);
            }
            index >>= 1;
        }
        
        return computed == root;
    }

    // =========================================================================
    // Generalized Index Computation
    // =========================================================================

    /**
     * @notice Computes the gindex for pending_consolidations[i].source_index from block_root
     * @param consolidationIndex Index in the pending_consolidations list
     * @return The generalized index
     * 
     * @dev Path from block_root:
     *      1. header.state_root (gindex 11 in header, depth 3)
     *      2. state.pending_consolidations (gindex 100 in state, depth 6)
     *      3. List wrapper: multiply by 2 for data node (not length mix-in)
     *      4. list_data[consolidationIndex] (depth 18 for PENDING_CONSOLIDATIONS_LIMIT=2^18)
     *      5. consolidation.source_index (gindex 8+0=8 → but field index 0 means gindex 2^1 + 0 = 2)
     *         Actually for depth 1: base is 2, field 0 gives gindex 2
     */
    function consolidationSourceGindex(uint64 consolidationIndex) internal pure returns (uint256) {
        // Start from block_root (gindex 1)
        // Navigate to state_root in header: gindex becomes 11
        uint256 gindex = STATE_ROOT_GINDEX_IN_HEADER;
        
        // Navigate to pending_consolidations in state
        // Concatenate: multiply by 2^STATE_DEPTH and add field gindex offset
        gindex = (gindex << STATE_DEPTH) | PENDING_CONSOLIDATIONS_FIELD_INDEX;
        
        // List structure: first go to data subtree (left child = *2)
        gindex = gindex << 1;
        
        // Navigate to element at consolidationIndex in list data tree
        gindex = (gindex << CONSOLIDATION_LIST_DEPTH) | uint256(consolidationIndex);
        
        // Navigate to source_index field (field 0) in PendingConsolidation
        gindex = (gindex << CONSOLIDATION_DEPTH) | SOURCE_INDEX_FIELD_INDEX;
        
        return gindex;
    }

    /**
     * @notice Computes the gindex for validators[i].withdrawal_credentials from block_root
     * @param validatorIndex Index in the validators list
     * @return The generalized index
     */
    function validatorCredentialsGindex(uint64 validatorIndex) internal pure returns (uint256) {
        // Navigate to state_root in header
        uint256 gindex = STATE_ROOT_GINDEX_IN_HEADER;
        
        // Navigate to validators in state
        gindex = (gindex << STATE_DEPTH) | VALIDATORS_FIELD_INDEX;
        
        // List structure: go to data subtree (left child)
        gindex = gindex << 1;
        
        // Navigate to element at validatorIndex in list data tree
        gindex = (gindex << VALIDATOR_LIST_DEPTH) | uint256(validatorIndex);
        
        // Navigate to withdrawal_credentials field (field 1) in Validator
        gindex = (gindex << VALIDATOR_DEPTH) | WITHDRAWAL_CREDENTIALS_FIELD_INDEX;
        
        return gindex;
    }

    /**
     * @notice Computes the gindex for validators[i].activation_epoch from block_root
     * @param validatorIndex Index in the validators list
     * @return The generalized index
     */
    function validatorActivationEpochGindex(uint64 validatorIndex) internal pure returns (uint256) {
        // Navigate to state_root in header
        uint256 gindex = STATE_ROOT_GINDEX_IN_HEADER;
        
        // Navigate to validators in state
        gindex = (gindex << STATE_DEPTH) | VALIDATORS_FIELD_INDEX;
        
        // List structure: go to data subtree (left child)
        gindex = gindex << 1;
        
        // Navigate to element at validatorIndex in list data tree
        gindex = (gindex << VALIDATOR_LIST_DEPTH) | uint256(validatorIndex);
        
        // Navigate to activation_epoch field (field 5) in Validator
        gindex = (gindex << VALIDATOR_DEPTH) | ACTIVATION_EPOCH_FIELD_INDEX;
        
        return gindex;
    }

    // =========================================================================
    // SSZ Encoding Helpers
    // =========================================================================

    /**
     * @notice Encodes a uint64 as SSZ little-endian bytes32
     * @param value The uint64 value to encode
     * @return The SSZ-encoded bytes32 (little-endian in first 8 bytes, rest zero)
     */
    function toLittleEndian64(uint64 value) internal pure returns (bytes32) {
        // Reverse the 8 bytes of the uint64
        uint64 reversed = 
            ((value & 0xFF00000000000000) >> 56) |
            ((value & 0x00FF000000000000) >> 40) |
            ((value & 0x0000FF0000000000) >> 24) |
            ((value & 0x000000FF00000000) >> 8) |
            ((value & 0x00000000FF000000) << 8) |
            ((value & 0x0000000000FF0000) << 24) |
            ((value & 0x000000000000FF00) << 40) |
            ((value & 0x00000000000000FF) << 56);
        
        // Place in first 8 bytes of bytes32 (SSZ pads with zeros on the right)
        return bytes32(uint256(reversed) << 192);
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /**
     * @notice Computes SHA256 of two concatenated bytes32 values using the precompile
     * @param a First 32 bytes
     * @param b Second 32 bytes
     * @return The SHA256 hash
     */
    function _sha256Pair(bytes32 a, bytes32 b) private view returns (bytes32) {
        bytes32 result;
        assembly {
            // Store a and b in memory
            let ptr := mload(0x40)
            mstore(ptr, a)
            mstore(add(ptr, 32), b)
            
            // Call SHA256 precompile (address 0x02)
            // Input: 64 bytes at ptr
            // Output: 32 bytes at ptr
            if iszero(staticcall(gas(), 0x02, ptr, 64, ptr, 32)) {
                revert(0, 0)
            }
            
            result := mload(ptr)
        }
        return result;
    }

    /**
     * @notice Computes floor(log2(x)) for x > 0
     * @param x The input value
     * @return The floor of log base 2
     */
    function _log2(uint256 x) private pure returns (uint256) {
        require(x > 0, "log2(0) undefined");
        uint256 result = 0;
        
        // Binary search for the highest set bit
        if (x >= 2**128) { x >>= 128; result += 128; }
        if (x >= 2**64) { x >>= 64; result += 64; }
        if (x >= 2**32) { x >>= 32; result += 32; }
        if (x >= 2**16) { x >>= 16; result += 16; }
        if (x >= 2**8) { x >>= 8; result += 8; }
        if (x >= 2**4) { x >>= 4; result += 4; }
        if (x >= 2**2) { x >>= 2; result += 2; }
        if (x >= 2**1) { result += 1; }
        
        return result;
    }
}
