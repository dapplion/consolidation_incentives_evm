// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/**
 * @title MockBeaconRootsOracle
 * @notice Mock implementation of the EIP-4788 beacon block root oracle for testing
 * @dev Deploy this contract and use vm.etch to place it at the EIP-4788 address
 */
contract MockBeaconRootsOracle {
    /// @notice Maps beacon timestamps to block roots
    mapping(uint256 => bytes32) public roots;

    /**
     * @notice Sets a beacon block root for a given timestamp
     * @param timestamp The beacon chain timestamp
     * @param root The beacon block root
     */
    function setRoot(uint256 timestamp, bytes32 root) external {
        roots[timestamp] = root;
    }

    /**
     * @notice Sets multiple roots at once
     * @param timestamps Array of timestamps
     * @param blockRoots Array of corresponding roots
     */
    function setRoots(uint256[] calldata timestamps, bytes32[] calldata blockRoots) external {
        require(timestamps.length == blockRoots.length, "Length mismatch");
        for (uint256 i = 0; i < timestamps.length; i++) {
            roots[timestamps[i]] = blockRoots[i];
        }
    }

    /**
     * @notice Fallback that mimics EIP-4788 behavior
     * @dev When called with a 32-byte timestamp, returns the corresponding root
     *      Returns empty data if no root exists (EIP-4788 reverts, but we return 0 for simplicity)
     */
    fallback(bytes calldata data) external returns (bytes memory) {
        require(data.length == 32, "Invalid input length");
        uint256 timestamp = abi.decode(data, (uint256));
        bytes32 root = roots[timestamp];
        return abi.encode(root);
    }
}
