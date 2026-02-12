// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ConsolidationIncentives} from "../src/ConsolidationIncentives.sol";
import {MockBeaconRootsOracle} from "./mocks/MockBeaconRootsOracle.sol";

/**
 * @title ConsolidationIncentivesTest
 * @notice Integration tests for the ConsolidationIncentives contract
 * @dev Tests will be expanded when test vectors from the Rust prover are available
 */
contract ConsolidationIncentivesTest is Test {
    ConsolidationIncentives public incentives;
    MockBeaconRootsOracle public mockOracle;

    address public owner = address(0x1);
    address public user = address(0x2);
    
    uint64 public constant MAX_EPOCH = 1000;
    uint256 public constant REWARD_AMOUNT = 1 ether;
    uint256 public constant MIN_CLAIM_DELAY = 1 hours;

    function setUp() public {
        // Set a reasonable block timestamp (avoid underflow in tests)
        vm.warp(1700000000); // Some time in 2023
        
        // Deploy mock beacon roots oracle
        mockOracle = new MockBeaconRootsOracle();
        
        // Etch the mock oracle at the EIP-4788 address
        vm.etch(0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02, address(mockOracle).code);
        
        // Store the mock oracle's storage at the EIP-4788 address
        // Note: We need to interact with the etched contract differently
        // For testing, we'll use vm.store to set up roots
        
        // Deploy implementation
        ConsolidationIncentives implementation = new ConsolidationIncentives();
        
        // Deploy proxy
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (owner, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        ERC1967Proxy proxy = new ERC1967Proxy(address(implementation), initData);
        incentives = ConsolidationIncentives(payable(address(proxy)));
        
        // Fund the contract
        vm.deal(address(incentives), 100 ether);
    }

    // =========================================================================
    // Initialization Tests
    // =========================================================================

    function test_initialize_setsParameters() public view {
        assertEq(incentives.maxEpoch(), MAX_EPOCH);
        assertEq(incentives.rewardAmount(), REWARD_AMOUNT);
        assertEq(incentives.minClaimDelay(), MIN_CLAIM_DELAY);
        assertEq(incentives.owner(), owner);
    }

    function test_initialize_cannotReinitialize() public {
        vm.expectRevert();
        incentives.initialize(owner, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY);
    }

    // =========================================================================
    // Receive Tests
    // =========================================================================

    function test_receive_acceptsFunding() public {
        uint256 balanceBefore = address(incentives).balance;
        
        vm.deal(user, 10 ether);
        vm.prank(user);
        (bool success,) = address(incentives).call{value: 1 ether}("");
        
        assertTrue(success);
        assertEq(address(incentives).balance, balanceBefore + 1 ether);
    }

    // =========================================================================
    // Admin Function Tests
    // =========================================================================

    function test_withdraw_onlyOwner() public {
        vm.prank(user);
        vm.expectRevert();
        incentives.withdraw(user, 1 ether);
    }

    function test_withdraw_success() public {
        uint256 withdrawAmount = 10 ether;
        uint256 ownerBalanceBefore = owner.balance;
        uint256 contractBalanceBefore = address(incentives).balance;
        
        vm.prank(owner);
        incentives.withdraw(owner, withdrawAmount);
        
        assertEq(owner.balance, ownerBalanceBefore + withdrawAmount);
        assertEq(address(incentives).balance, contractBalanceBefore - withdrawAmount);
    }

    function test_withdraw_insufficientBalance() public {
        vm.prank(owner);
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InsufficientBalance.selector,
                1000 ether,
                address(incentives).balance
            )
        );
        incentives.withdraw(owner, 1000 ether);
    }

    function test_withdraw_emitsEvent() public {
        vm.prank(owner);
        vm.expectEmit(true, false, false, true);
        emit ConsolidationIncentives.Withdrawn(owner, 1 ether);
        incentives.withdraw(owner, 1 ether);
    }

    // =========================================================================
    // ClaimReward Validation Tests (without real proofs)
    // These test the validation logic before proof verification
    // =========================================================================

    function test_rewarded_initiallyFalse() public view {
        // Verify that validators are not marked as rewarded initially
        assertFalse(incentives.rewarded(0));
        assertFalse(incentives.rewarded(42));
        assertFalse(incentives.rewarded(type(uint64).max));
    }

    function test_claimReward_timestampTooRecent_reverts() public {
        uint64 beaconTimestamp = uint64(block.timestamp); // Current time = too recent
        
        bytes32[] memory emptyProof29 = new bytes32[](29);
        bytes32[] memory emptyProof53 = new bytes32[](53);
        
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.TimestampTooRecent.selector,
                beaconTimestamp,
                block.timestamp,
                MIN_CLAIM_DELAY
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            0,
            42,
            100,
            bytes32(uint256(0x01) << 248),
            emptyProof29,
            emptyProof53,
            emptyProof53
        );
    }

    function test_claimReward_beaconRootNotFound_reverts() public {
        uint64 beaconTimestamp = uint64(block.timestamp - MIN_CLAIM_DELAY - 1);
        
        bytes32[] memory emptyProof29 = new bytes32[](29);
        bytes32[] memory emptyProof53 = new bytes32[](53);
        
        // Oracle returns 0 for this timestamp (no root set)
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.BeaconRootNotFound.selector,
                beaconTimestamp
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            0,
            42,
            100,
            bytes32(uint256(0x01) << 248),
            emptyProof29,
            emptyProof53,
            emptyProof53
        );
    }

    function test_claimReward_wrongConsolidationProofLength_reverts() public {
        uint64 beaconTimestamp = uint64(block.timestamp - MIN_CLAIM_DELAY - 1);
        
        // Set a beacon root
        _setBeaconRoot(beaconTimestamp, keccak256("testRoot"));
        
        bytes32[] memory wrongLengthProof = new bytes32[](10); // Should be 29
        bytes32[] memory emptyProof53 = new bytes32[](53);
        
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProofLength.selector,
                10,
                29
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            0,
            42,
            100,
            bytes32(uint256(0x01) << 248),
            wrongLengthProof,
            emptyProof53,
            emptyProof53
        );
    }

    function test_claimReward_wrongCredentialsProofLength_reverts() public {
        uint64 beaconTimestamp = uint64(block.timestamp - MIN_CLAIM_DELAY - 1);
        _setBeaconRoot(beaconTimestamp, keccak256("testRoot"));
        
        bytes32[] memory proof29 = new bytes32[](29);
        bytes32[] memory wrongLengthProof = new bytes32[](20); // Should be 53
        bytes32[] memory proof53 = new bytes32[](53);
        
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProofLength.selector,
                20,
                53
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            0,
            42,
            100,
            bytes32(uint256(0x01) << 248),
            proof29,
            wrongLengthProof,
            proof53
        );
    }

    function test_claimReward_wrongActivationProofLength_reverts() public {
        uint64 beaconTimestamp = uint64(block.timestamp - MIN_CLAIM_DELAY - 1);
        _setBeaconRoot(beaconTimestamp, keccak256("testRoot"));
        
        bytes32[] memory proof29 = new bytes32[](29);
        bytes32[] memory proof53 = new bytes32[](53);
        bytes32[] memory wrongLengthProof = new bytes32[](30); // Should be 53
        
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProofLength.selector,
                30,
                53
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            0,
            42,
            100,
            bytes32(uint256(0x01) << 248),
            proof29,
            proof53,
            wrongLengthProof
        );
    }

    // =========================================================================
    // Upgrade Tests
    // =========================================================================

    function test_upgrade_onlyOwner() public {
        ConsolidationIncentives newImpl = new ConsolidationIncentives();
        
        vm.prank(user);
        vm.expectRevert();
        incentives.upgradeToAndCall(address(newImpl), "");
    }

    function test_upgrade_success() public {
        ConsolidationIncentives newImpl = new ConsolidationIncentives();
        
        vm.prank(owner);
        incentives.upgradeToAndCall(address(newImpl), "");
        
        // Contract should still work after upgrade
        assertEq(incentives.maxEpoch(), MAX_EPOCH);
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    function _setBeaconRoot(uint64 timestamp, bytes32 root) internal {
        // The MockBeaconRootsOracle uses a mapping(uint256 => bytes32) at slot 0
        // Compute the storage slot for roots[timestamp]
        bytes32 slot = keccak256(abi.encode(uint256(timestamp), uint256(0)));
        vm.store(0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02, slot, root);
    }
}
