// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {SSZMerkleVerifier} from "../src/lib/SSZMerkleVerifier.sol";

/**
 * @title SSZMerkleVerifierTest
 * @notice Unit tests for the SSZMerkleVerifier library
 */
contract SSZMerkleVerifierTest is Test {
    // =========================================================================
    // Test Helpers
    // =========================================================================

    /// @dev Wrapper to call library functions (since library is internal)
    function verifyProof(
        bytes32 root,
        bytes32 leaf,
        bytes32[] calldata proof,
        uint256 gindex
    ) public view returns (bool) {
        return SSZMerkleVerifier.verifyProof(root, leaf, proof, gindex);
    }

    function consolidationSourceGindex(uint64 consolidationIndex) public pure returns (uint256) {
        return SSZMerkleVerifier.consolidationSourceGindex(consolidationIndex);
    }

    function validatorCredentialsGindex(uint64 validatorIndex) public pure returns (uint256) {
        return SSZMerkleVerifier.validatorCredentialsGindex(validatorIndex);
    }

    function validatorActivationEpochGindex(uint64 validatorIndex) public pure returns (uint256) {
        return SSZMerkleVerifier.validatorActivationEpochGindex(validatorIndex);
    }

    function toLittleEndian64(uint64 value) public pure returns (bytes32) {
        return SSZMerkleVerifier.toLittleEndian64(value);
    }

    // =========================================================================
    // toLittleEndian64 Tests
    // =========================================================================

    function test_toLittleEndian64_zero() public pure {
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(0);
        assertEq(result, bytes32(0));
    }

    function test_toLittleEndian64_one() public pure {
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(1);
        // 1 in little-endian is 0x01 in the first byte
        assertEq(result[0], bytes1(0x01));
        for (uint256 i = 1; i < 32; i++) {
            assertEq(result[i], bytes1(0x00));
        }
    }

    function test_toLittleEndian64_256() public pure {
        // 256 = 0x100 in big-endian = 0x0001 in little-endian (second byte is 0x01)
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(256);
        assertEq(result[0], bytes1(0x00));
        assertEq(result[1], bytes1(0x01));
        for (uint256 i = 2; i < 32; i++) {
            assertEq(result[i], bytes1(0x00));
        }
    }

    function test_toLittleEndian64_maxValue() public pure {
        // type(uint64).max = 0xFFFFFFFFFFFFFFFF
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(type(uint64).max);
        for (uint256 i = 0; i < 8; i++) {
            assertEq(result[i], bytes1(0xFF));
        }
        for (uint256 i = 8; i < 32; i++) {
            assertEq(result[i], bytes1(0x00));
        }
    }

    function test_toLittleEndian64_knownValue() public pure {
        // Test with epoch 12345 = 0x3039
        // Little-endian: 0x39 0x30 0x00 0x00 0x00 0x00 0x00 0x00
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(12345);
        assertEq(result[0], bytes1(0x39));
        assertEq(result[1], bytes1(0x30));
        for (uint256 i = 2; i < 32; i++) {
            assertEq(result[i], bytes1(0x00));
        }
    }

    // =========================================================================
    // Gindex Computation Tests
    // =========================================================================

    function test_consolidationSourceGindex_index0() public pure {
        uint256 gindex = SSZMerkleVerifier.consolidationSourceGindex(0);
        // Should be non-zero and reasonable
        assertTrue(gindex > 0);
        // The depth should be 29 (CONSOLIDATION_PROOF_LENGTH)
        // log2(gindex) == 29, so gindex is between 2^29 and 2^30-1
        assertTrue(gindex >= 2**29);
        assertTrue(gindex < 2**30);
    }

    function test_consolidationSourceGindex_index1() public pure {
        uint256 gindex0 = SSZMerkleVerifier.consolidationSourceGindex(0);
        uint256 gindex1 = SSZMerkleVerifier.consolidationSourceGindex(1);
        // Index 1 should differ from index 0
        assertTrue(gindex0 != gindex1);
        // Both should have same depth
        assertTrue(gindex1 >= 2**29);
        assertTrue(gindex1 < 2**30);
    }

    function test_validatorCredentialsGindex_index0() public pure {
        uint256 gindex = SSZMerkleVerifier.validatorCredentialsGindex(0);
        assertTrue(gindex > 0);
        // Depth should be 53 (VALIDATOR_PROOF_LENGTH)
        assertTrue(gindex >= 2**53);
        assertTrue(gindex < 2**54);
    }

    function test_validatorActivationEpochGindex_index0() public pure {
        uint256 gindex = SSZMerkleVerifier.validatorActivationEpochGindex(0);
        assertTrue(gindex > 0);
        // Depth should be 53 (VALIDATOR_PROOF_LENGTH)
        assertTrue(gindex >= 2**53);
        assertTrue(gindex < 2**54);
    }

    function test_validatorGindexes_differentFields() public pure {
        // withdrawal_credentials and activation_epoch should have different gindexes
        // for the same validator index (different field indices)
        uint256 credGindex = SSZMerkleVerifier.validatorCredentialsGindex(42);
        uint256 epochGindex = SSZMerkleVerifier.validatorActivationEpochGindex(42);
        assertTrue(credGindex != epochGindex);
    }

    // =========================================================================
    // Proof Verification Tests
    // =========================================================================

    function test_verifyProof_gindexZero_returnsFalse() public view {
        bytes32 root = keccak256("root");
        bytes32 leaf = keccak256("leaf");
        bytes32[] memory proof = new bytes32[](0);
        
        bool result = this.verifyProof(root, leaf, proof, 0);
        assertFalse(result);
    }

    function test_verifyProof_gindexOne_leafEqualsRoot() public view {
        bytes32 root = keccak256("data");
        bytes32[] memory proof = new bytes32[](0);
        
        // When gindex is 1, leaf must equal root
        bool result = this.verifyProof(root, root, proof, 1);
        assertTrue(result);
    }

    function test_verifyProof_gindexOne_leafNotEqualRoot_returnsFalse() public view {
        bytes32 root = keccak256("root");
        bytes32 leaf = keccak256("different");
        bytes32[] memory proof = new bytes32[](0);
        
        bool result = this.verifyProof(root, leaf, proof, 1);
        assertFalse(result);
    }

    function test_verifyProof_gindexOne_withProof_returnsFalse() public view {
        bytes32 root = keccak256("root");
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = keccak256("sibling");
        
        // Gindex 1 should have empty proof
        bool result = this.verifyProof(root, root, proof, 1);
        assertFalse(result);
    }

    function test_verifyProof_depth1_leftChild() public view {
        // Build a simple tree with 2 leaves
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 root = _sha256(abi.encodePacked(left, right));
        
        // Gindex 2 is left child (depth 1)
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = right;
        
        bool result = this.verifyProof(root, left, proof, 2);
        assertTrue(result);
    }

    function test_verifyProof_depth1_rightChild() public view {
        // Build a simple tree with 2 leaves
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 root = _sha256(abi.encodePacked(left, right));
        
        // Gindex 3 is right child (depth 1)
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = left;
        
        bool result = this.verifyProof(root, right, proof, 3);
        assertTrue(result);
    }

    function test_verifyProof_depth2() public view {
        // Build a tree with 4 leaves
        bytes32 leaf0 = keccak256("leaf0");
        bytes32 leaf1 = keccak256("leaf1");
        bytes32 leaf2 = keccak256("leaf2");
        bytes32 leaf3 = keccak256("leaf3");
        
        bytes32 node0 = _sha256(abi.encodePacked(leaf0, leaf1));
        bytes32 node1 = _sha256(abi.encodePacked(leaf2, leaf3));
        bytes32 root = _sha256(abi.encodePacked(node0, node1));
        
        // Verify leaf2 at gindex 6 (binary: 110, path: right then left)
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf3;  // sibling at depth 2
        proof[1] = node0;  // sibling at depth 1
        
        bool result = this.verifyProof(root, leaf2, proof, 6);
        assertTrue(result);
    }

    function test_verifyProof_wrongLeaf_returnsFalse() public view {
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 root = _sha256(abi.encodePacked(left, right));
        
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = right;
        
        // Try to verify wrong leaf
        bytes32 wrongLeaf = keccak256("wrong");
        bool result = this.verifyProof(root, wrongLeaf, proof, 2);
        assertFalse(result);
    }

    function test_verifyProof_wrongRoot_returnsFalse() public view {
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 wrongRoot = keccak256("wrongRoot");
        
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = right;
        
        bool result = this.verifyProof(wrongRoot, left, proof, 2);
        assertFalse(result);
    }

    function test_verifyProof_wrongProofLength_returnsFalse() public view {
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 root = _sha256(abi.encodePacked(left, right));
        
        // Gindex 2 requires depth 1 (proof length 1), but we provide 2
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = right;
        proof[1] = keccak256("extra");
        
        bool result = this.verifyProof(root, left, proof, 2);
        assertFalse(result);
    }

    function test_verifyProof_emptyProof_gindexNotOne_returnsFalse() public view {
        bytes32 root = keccak256("root");
        bytes32 leaf = keccak256("leaf");
        bytes32[] memory proof = new bytes32[](0);
        
        // Gindex 2 requires proof length 1
        bool result = this.verifyProof(root, leaf, proof, 2);
        assertFalse(result);
    }

    function test_verifyProof_wrongSibling_returnsFalse() public view {
        bytes32 left = keccak256("left");
        bytes32 right = keccak256("right");
        bytes32 root = _sha256(abi.encodePacked(left, right));
        
        bytes32[] memory proof = new bytes32[](1);
        proof[0] = keccak256("wrongSibling");
        
        bool result = this.verifyProof(root, left, proof, 2);
        assertFalse(result);
    }

    // =========================================================================
    // Fuzz Tests
    // =========================================================================

    function testFuzz_toLittleEndian64_reversible(uint64 value) public pure {
        bytes32 result = SSZMerkleVerifier.toLittleEndian64(value);
        
        // Extract first 8 bytes and reverse them back
        uint64 recovered = 0;
        for (uint256 i = 0; i < 8; i++) {
            recovered |= uint64(uint8(result[i])) << (i * 8);
        }
        
        assertEq(recovered, value);
    }

    function testFuzz_consolidationGindex_depth(uint64 index) public pure {
        // Bound index to reasonable range
        vm.assume(index < 2**18); // PENDING_CONSOLIDATIONS_LIMIT
        
        uint256 gindex = SSZMerkleVerifier.consolidationSourceGindex(index);
        
        // Verify depth is 29 (gindex is in [2^29, 2^30))
        assertTrue(gindex >= 2**29);
        assertTrue(gindex < 2**30);
    }

    function testFuzz_validatorGindex_depth(uint64 index) public pure {
        // Bound index to reasonable range  
        vm.assume(index < 2**20); // Reasonable validator count
        
        uint256 credGindex = SSZMerkleVerifier.validatorCredentialsGindex(index);
        uint256 epochGindex = SSZMerkleVerifier.validatorActivationEpochGindex(index);
        
        // Verify depth is 53 (gindex is in [2^53, 2^54))
        assertTrue(credGindex >= 2**53);
        assertTrue(credGindex < 2**54);
        assertTrue(epochGindex >= 2**53);
        assertTrue(epochGindex < 2**54);
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    function _sha256(bytes memory data) internal view returns (bytes32) {
        (bool success, bytes memory result) = address(0x02).staticcall(data);
        require(success, "SHA256 failed");
        return abi.decode(result, (bytes32));
    }
}
