// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Test, console2} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ConsolidationIncentives} from "../src/ConsolidationIncentives.sol";
import {MockBeaconRootsOracle} from "./mocks/MockBeaconRootsOracle.sol";
import {SSZMerkleVerifier} from "../src/lib/SSZMerkleVerifier.sol";

/**
 * @title ConsolidationIncentivesVectorsTest
 * @notice Integration tests using real SSZ Merkle proofs from the Rust prover
 * @dev Loads test vectors from contracts/test-vectors/test_vectors.json
 */
contract ConsolidationIncentivesVectorsTest is Test {
    ConsolidationIncentives public incentives;

    address public owner = address(0xBEEF);

    uint256 public constant REWARD_AMOUNT = 1 ether;
    uint256 public constant MIN_CLAIM_DELAY = 1 hours;

    // Loaded from test vectors
    bytes32 public blockRoot;
    uint64 public beaconTimestamp;
    uint64 public maxEpoch;

    // Parsed claims
    struct Claim {
        uint64 consolidationIndex;
        uint64 sourceIndex;
        uint64 activationEpoch;
        bytes32 sourceCredentials;
        bytes32[] proofConsolidation;
        bytes32[] proofCredentials;
        bytes32[] proofActivationEpoch;
        address expectedRecipient;
    }

    struct InvalidClaim {
        string description;
        uint64 consolidationIndex;
        uint64 sourceIndex;
        uint64 activationEpoch;
        bytes32 sourceCredentials;
        bytes32[] proofConsolidation;
        bytes32[] proofCredentials;
        bytes32[] proofActivationEpoch;
        string expectedError;
    }

    Claim[] internal claims;
    InvalidClaim[] internal invalidClaims;

    function setUp() public {
        // Load test vectors
        string memory json = vm.readFile("test-vectors/test_vectors.json");

        blockRoot = vm.parseJsonBytes32(json, ".block_root");
        beaconTimestamp = uint64(vm.parseJsonUint(json, ".beacon_timestamp"));
        maxEpoch = uint64(vm.parseJsonUint(json, ".max_epoch"));

        // Parse valid claims
        bytes memory claimsRaw = vm.parseJson(json, ".claims");
        bytes[] memory claimEntries = abi.decode(claimsRaw, (bytes[]));

        for (uint256 i = 0; i < claimEntries.length; i++) {
            string memory prefix = string.concat(".claims[", vm.toString(i), "]");

            Claim memory c;
            c.consolidationIndex = uint64(vm.parseJsonUint(json, string.concat(prefix, ".consolidation_index")));
            c.sourceIndex = uint64(vm.parseJsonUint(json, string.concat(prefix, ".source_index")));
            c.activationEpoch = uint64(vm.parseJsonUint(json, string.concat(prefix, ".activation_epoch")));
            c.sourceCredentials = vm.parseJsonBytes32(json, string.concat(prefix, ".source_credentials"));
            c.proofConsolidation = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_consolidation"));
            c.proofCredentials = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_credentials"));
            c.proofActivationEpoch = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_activation_epoch"));
            c.expectedRecipient = vm.parseJsonAddress(json, string.concat(prefix, ".expected_recipient"));

            claims.push(c);
        }

        // Parse invalid claims
        bytes memory invalidRaw = vm.parseJson(json, ".invalid_claims");
        bytes[] memory invalidEntries = abi.decode(invalidRaw, (bytes[]));

        for (uint256 i = 0; i < invalidEntries.length; i++) {
            string memory prefix = string.concat(".invalid_claims[", vm.toString(i), "]");

            InvalidClaim memory ic;
            ic.description = vm.parseJsonString(json, string.concat(prefix, ".description"));
            ic.consolidationIndex = uint64(vm.parseJsonUint(json, string.concat(prefix, ".consolidation_index")));
            ic.sourceIndex = uint64(vm.parseJsonUint(json, string.concat(prefix, ".source_index")));
            ic.activationEpoch = uint64(vm.parseJsonUint(json, string.concat(prefix, ".activation_epoch")));
            ic.sourceCredentials = vm.parseJsonBytes32(json, string.concat(prefix, ".source_credentials"));
            ic.proofConsolidation = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_consolidation"));
            ic.proofCredentials = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_credentials"));
            ic.proofActivationEpoch = vm.parseJsonBytes32Array(json, string.concat(prefix, ".proof_activation_epoch"));
            ic.expectedError = vm.parseJsonString(json, string.concat(prefix, ".expected_error"));

            invalidClaims.push(ic);
        }

        // Set up contract
        vm.warp(beaconTimestamp + MIN_CLAIM_DELAY + 1);

        // Deploy mock oracle and etch at EIP-4788 address
        MockBeaconRootsOracle mockOracle = new MockBeaconRootsOracle();
        vm.etch(0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02, address(mockOracle).code);

        // Store the block root in the oracle
        _setBeaconRoot(beaconTimestamp, blockRoot);

        // Deploy contract via proxy
        ConsolidationIncentives implementation = new ConsolidationIncentives();
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (owner, maxEpoch, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        ERC1967Proxy proxy = new ERC1967Proxy(address(implementation), initData);
        incentives = ConsolidationIncentives(payable(address(proxy)));

        // Fund the contract generously
        vm.deal(address(incentives), 1000 ether);
    }

    // =========================================================================
    // Test Vector Validation
    // =========================================================================

    function test_vectorsLoaded() public view {
        assertGt(claims.length, 0, "No valid claims loaded");
        assertGt(invalidClaims.length, 0, "No invalid claims loaded");
        assertEq(claims.length, 4, "Expected 4 valid claims");
        assertEq(invalidClaims.length, 9, "Expected 9 invalid claims");
    }

    function test_proofLengths() public view {
        for (uint256 i = 0; i < claims.length; i++) {
            assertEq(
                claims[i].proofConsolidation.length,
                29,
                string.concat("Claim ", vm.toString(i), " consolidation proof length")
            );
            assertEq(
                claims[i].proofCredentials.length,
                53,
                string.concat("Claim ", vm.toString(i), " credentials proof length")
            );
            assertEq(
                claims[i].proofActivationEpoch.length,
                53,
                string.concat("Claim ", vm.toString(i), " activation proof length")
            );
        }
    }

    // =========================================================================
    // Happy Path Tests
    // =========================================================================

    function test_claimReward_claim0_success() public {
        _testClaimSuccess(0);
    }

    function test_claimReward_claim1_success() public {
        _testClaimSuccess(1);
    }

    function test_claimReward_claim2_0x02credentials() public {
        // Claim 2 uses 0x02 credentials (compounding)
        Claim memory c = claims[2];
        assertEq(uint8(c.sourceCredentials[0]), 0x02, "Should be 0x02 credentials");
        _testClaimSuccess(2);
    }

    function test_claimReward_claim3_success() public {
        _testClaimSuccess(3);
    }

    function test_claimReward_multipleValidators() public {
        // Claim all 4 valid claims sequentially
        for (uint256 i = 0; i < claims.length; i++) {
            Claim memory c = claims[i];

            uint256 recipientBalanceBefore = c.expectedRecipient.balance;

            incentives.claimReward(
                beaconTimestamp,
                c.consolidationIndex,
                c.sourceIndex,
                c.activationEpoch,
                c.sourceCredentials,
                c.proofConsolidation,
                c.proofCredentials,
                c.proofActivationEpoch
            );

            assertEq(
                c.expectedRecipient.balance,
                recipientBalanceBefore + REWARD_AMOUNT,
                string.concat("Claim ", vm.toString(i), " reward not received")
            );
            assertTrue(
                incentives.rewarded(c.sourceIndex),
                string.concat("Claim ", vm.toString(i), " not marked rewarded")
            );
        }
    }

    function test_claimReward_emitsEvent() public {
        Claim memory c = claims[0];

        vm.expectEmit(true, true, false, true);
        emit ConsolidationIncentives.RewardClaimed(
            c.sourceIndex,
            c.expectedRecipient,
            REWARD_AMOUNT
        );

        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );
    }

    // =========================================================================
    // Double-Claim Tests
    // =========================================================================

    function test_claimReward_doubleClaim_reverts() public {
        Claim memory c = claims[0];

        // First claim succeeds
        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );

        // Second claim reverts
        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.AlreadyClaimed.selector,
                c.sourceIndex
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );
    }

    // =========================================================================
    // Eligibility Tests (using invalid claims from test vectors)
    // =========================================================================

    function test_claimReward_activationEpochTooHigh_reverts() public {
        // Invalid claim 0: activation_epoch == maxEpoch
        InvalidClaim memory ic = invalidClaims[0];
        assertEq(keccak256(bytes(ic.expectedError)), keccak256(bytes("NotEligible")));

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.NotEligible.selector,
                ic.activationEpoch,
                maxEpoch
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_0x00credentials_reverts() public {
        // Invalid claim 1: BLS credentials (0x00 prefix)
        InvalidClaim memory ic = invalidClaims[1];
        assertEq(keccak256(bytes(ic.expectedError)), keccak256(bytes("InvalidCredentialsPrefix")));

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidCredentialsPrefix.selector,
                ic.sourceCredentials[0]
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    // =========================================================================
    // Invalid Proof Tests
    // =========================================================================

    function test_claimReward_tamperedConsolidationProof_reverts() public {
        // Invalid claim 2: tampered consolidation proof
        InvalidClaim memory ic = invalidClaims[2];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "consolidation"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_tamperedCredentialsProof_reverts() public {
        // Invalid claim 3: tampered credentials proof
        InvalidClaim memory ic = invalidClaims[3];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "credentials"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_tamperedActivationEpochProof_reverts() public {
        // Invalid claim 4: tampered activation epoch proof
        InvalidClaim memory ic = invalidClaims[4];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "activationEpoch"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_wrongSourceIndex_reverts() public {
        // Invalid claim 5: wrong source_index (proof for different validator)
        InvalidClaim memory ic = invalidClaims[5];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "consolidation"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_wrongCredentials_reverts() public {
        // Invalid claim 6: wrong credentials value
        InvalidClaim memory ic = invalidClaims[6];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "credentials"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_wrongActivationEpoch_reverts() public {
        // Invalid claim 7: wrong activation_epoch value
        InvalidClaim memory ic = invalidClaims[7];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProof.selector,
                "activationEpoch"
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    function test_claimReward_swappedProofs_reverts() public {
        // Invalid claim 8: consolidation proof used as credentials proof (wrong length)
        InvalidClaim memory ic = invalidClaims[8];

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InvalidProofLength.selector,
                ic.proofCredentials.length,
                53
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            ic.consolidationIndex,
            ic.sourceIndex,
            ic.activationEpoch,
            ic.sourceCredentials,
            ic.proofConsolidation,
            ic.proofCredentials,
            ic.proofActivationEpoch
        );
    }

    // =========================================================================
    // Finality Delay Tests
    // =========================================================================

    function test_claimReward_timestampTooRecent_reverts() public {
        Claim memory c = claims[0];

        // Warp to just before the delay expires
        vm.warp(beaconTimestamp + MIN_CLAIM_DELAY - 1);

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
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );
    }

    function test_claimReward_timestampExactDelay_succeeds() public {
        Claim memory c = claims[0];

        // Warp to exactly the delay
        vm.warp(beaconTimestamp + MIN_CLAIM_DELAY);

        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );

        assertTrue(incentives.rewarded(c.sourceIndex));
    }

    function test_claimReward_beaconRootNotFound_reverts() public {
        Claim memory c = claims[0];

        // Use a timestamp that has no root in oracle, but far enough back
        // that finality delay passes (otherwise TimestampTooRecent fires first)
        uint64 badTimestamp = beaconTimestamp - 1;

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.BeaconRootNotFound.selector,
                badTimestamp
            )
        );
        incentives.claimReward(
            badTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );
    }

    // =========================================================================
    // Funding Tests
    // =========================================================================

    function test_claimReward_insufficientBalance_reverts() public {
        Claim memory c = claims[0];

        // Drain the contract
        vm.prank(owner);
        incentives.withdraw(owner, address(incentives).balance);

        vm.expectRevert(
            abi.encodeWithSelector(
                ConsolidationIncentives.InsufficientBalance.selector,
                REWARD_AMOUNT,
                0
            )
        );
        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    function _testClaimSuccess(uint256 claimIndex) internal {
        Claim memory c = claims[claimIndex];

        uint256 recipientBalanceBefore = c.expectedRecipient.balance;
        uint256 contractBalanceBefore = address(incentives).balance;

        assertFalse(incentives.rewarded(c.sourceIndex), "Should not be rewarded yet");

        incentives.claimReward(
            beaconTimestamp,
            c.consolidationIndex,
            c.sourceIndex,
            c.activationEpoch,
            c.sourceCredentials,
            c.proofConsolidation,
            c.proofCredentials,
            c.proofActivationEpoch
        );

        assertTrue(incentives.rewarded(c.sourceIndex), "Should be marked as rewarded");
        assertEq(
            c.expectedRecipient.balance,
            recipientBalanceBefore + REWARD_AMOUNT,
            "Recipient should receive reward"
        );
        assertEq(
            address(incentives).balance,
            contractBalanceBefore - REWARD_AMOUNT,
            "Contract balance should decrease"
        );
    }

    function _setBeaconRoot(uint64 timestamp, bytes32 root) internal {
        bytes32 slot = keccak256(abi.encode(uint256(timestamp), uint256(0)));
        vm.store(0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02, slot, root);
    }
}
