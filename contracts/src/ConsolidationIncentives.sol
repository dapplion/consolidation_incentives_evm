// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {SSZMerkleVerifier} from "./lib/SSZMerkleVerifier.sol";

/**
 * @title ConsolidationIncentives
 * @notice Incentivizes Gnosis Chain validator consolidations (EIP-7251) by paying rewards
 *         to validators who consolidate before a deadline epoch.
 * @dev Uses EIP-4788 beacon block roots and SSZ Merkle proofs to verify consolidations on-chain.
 * 
 * To claim a reward, a validator must prove:
 * 1. They appear in the pending_consolidations list (as source)
 * 2. Their withdrawal credentials (0x01 or 0x02 prefix)
 * 3. Their activation_epoch is before maxEpoch
 * 
 * The reward is sent to the address encoded in the withdrawal credentials.
 */
contract ConsolidationIncentives is UUPSUpgradeable, OwnableUpgradeable {
    using SSZMerkleVerifier for bytes32[];

    // =========================================================================
    // Constants
    // =========================================================================

    /// @notice EIP-4788 Beacon Block Root Oracle address (same on all chains)
    address public constant EIP4788_ORACLE = 0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02;

    /// @notice Expected proof length for consolidation proofs
    uint256 public constant CONSOLIDATION_PROOF_LENGTH = SSZMerkleVerifier.CONSOLIDATION_PROOF_LENGTH;

    /// @notice Expected proof length for validator field proofs
    uint256 public constant VALIDATOR_PROOF_LENGTH = SSZMerkleVerifier.VALIDATOR_PROOF_LENGTH;

    // =========================================================================
    // Storage
    // =========================================================================

    /// @notice Maximum epoch (exclusive) for eligibility - validators must have activated before this
    uint64 public maxEpoch;

    /// @notice Fixed xDAI reward amount per successful claim
    uint256 public rewardAmount;

    /// @notice Minimum delay (seconds) between beacon timestamp and claim time (finality safety)
    uint256 public minClaimDelay;

    /// @notice Tracks which validator indices have already claimed rewards
    mapping(uint64 => bool) public rewarded;

    // =========================================================================
    // Events
    // =========================================================================

    /// @notice Emitted when a reward is successfully claimed
    /// @param sourceIndex The validator index that was consolidated
    /// @param recipient The address that received the reward
    /// @param amount The reward amount in wei
    event RewardClaimed(uint64 indexed sourceIndex, address indexed recipient, uint256 amount);

    /// @notice Emitted when the owner withdraws funds
    /// @param to The recipient address
    /// @param amount The amount withdrawn
    event Withdrawn(address indexed to, uint256 amount);

    // =========================================================================
    // Errors
    // =========================================================================

    /// @notice The validator has already claimed their reward
    error AlreadyClaimed(uint64 sourceIndex);

    /// @notice The beacon timestamp is too recent (finality not guaranteed)
    error TimestampTooRecent(uint64 beaconTimestamp, uint256 currentTime, uint256 requiredDelay);

    /// @notice EIP-4788 oracle returned zero or call failed
    error BeaconRootNotFound(uint64 beaconTimestamp);

    /// @notice Proof length doesn't match expected length
    error InvalidProofLength(uint256 provided, uint256 expected);

    /// @notice Merkle proof verification failed
    error InvalidProof(string proofType);

    /// @notice Validator activation epoch is not before maxEpoch
    error NotEligible(uint64 activationEpoch, uint64 maxEpoch);

    /// @notice Withdrawal credentials don't have 0x01 or 0x02 prefix
    error InvalidCredentialsPrefix(bytes1 prefix);

    /// @notice Contract doesn't have enough balance to pay reward
    error InsufficientBalance(uint256 required, uint256 available);

    /// @notice Reward transfer failed
    error TransferFailed(address recipient, uint256 amount);

    // =========================================================================
    // Initialization
    // =========================================================================

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /**
     * @notice Initializes the contract
     * @param _owner The owner address (can upgrade and withdraw)
     * @param _maxEpoch Maximum epoch for eligibility (exclusive)
     * @param _rewardAmount Reward per claim in wei
     * @param _minClaimDelay Minimum seconds between beacon timestamp and claim
     */
    function initialize(
        address _owner,
        uint64 _maxEpoch,
        uint256 _rewardAmount,
        uint256 _minClaimDelay
    ) external initializer {
        __Ownable_init(_owner);
        
        maxEpoch = _maxEpoch;
        rewardAmount = _rewardAmount;
        minClaimDelay = _minClaimDelay;
    }

    // =========================================================================
    // Main Claim Function
    // =========================================================================

    /**
     * @notice Claims a reward for a validated consolidation
     * @param beaconTimestamp The beacon chain timestamp for EIP-4788 lookup
     * @param consolidationIndex Index in the pending_consolidations list
     * @param sourceIndex The source validator index being consolidated
     * @param activationEpoch The source validator's activation epoch
     * @param sourceCredentials The source validator's withdrawal credentials
     * @param proofConsolidation Merkle proof for pending_consolidations[consolidationIndex].source_index
     * @param proofCredentials Merkle proof for validators[sourceIndex].withdrawal_credentials
     * @param proofActivationEpoch Merkle proof for validators[sourceIndex].activation_epoch
     */
    function claimReward(
        uint64 beaconTimestamp,
        uint64 consolidationIndex,
        uint64 sourceIndex,
        uint64 activationEpoch,
        bytes32 sourceCredentials,
        bytes32[] calldata proofConsolidation,
        bytes32[] calldata proofCredentials,
        bytes32[] calldata proofActivationEpoch
    ) external {
        // 1. Check not already claimed
        if (rewarded[sourceIndex]) {
            revert AlreadyClaimed(sourceIndex);
        }

        // 2. Check finality delay
        if (block.timestamp < beaconTimestamp + minClaimDelay) {
            revert TimestampTooRecent(beaconTimestamp, block.timestamp, minClaimDelay);
        }

        // 3. Get beacon block root from EIP-4788 oracle
        bytes32 blockRoot = _getBeaconBlockRoot(beaconTimestamp);
        if (blockRoot == bytes32(0)) {
            revert BeaconRootNotFound(beaconTimestamp);
        }

        // 4. Verify proof lengths
        if (proofConsolidation.length != CONSOLIDATION_PROOF_LENGTH) {
            revert InvalidProofLength(proofConsolidation.length, CONSOLIDATION_PROOF_LENGTH);
        }
        if (proofCredentials.length != VALIDATOR_PROOF_LENGTH) {
            revert InvalidProofLength(proofCredentials.length, VALIDATOR_PROOF_LENGTH);
        }
        if (proofActivationEpoch.length != VALIDATOR_PROOF_LENGTH) {
            revert InvalidProofLength(proofActivationEpoch.length, VALIDATOR_PROOF_LENGTH);
        }

        // 5. Verify consolidation proof: proves sourceIndex is at pending_consolidations[consolidationIndex].source_index
        bytes32 sourceIndexLeaf = SSZMerkleVerifier.toLittleEndian64(sourceIndex);
        uint256 consolidationGindex = SSZMerkleVerifier.consolidationSourceGindex(consolidationIndex);
        if (!SSZMerkleVerifier.verifyProof(blockRoot, sourceIndexLeaf, proofConsolidation, consolidationGindex)) {
            revert InvalidProof("consolidation");
        }

        // 6. Verify credentials proof: proves sourceCredentials is at validators[sourceIndex].withdrawal_credentials
        uint256 credentialsGindex = SSZMerkleVerifier.validatorCredentialsGindex(sourceIndex);
        if (!SSZMerkleVerifier.verifyProof(blockRoot, sourceCredentials, proofCredentials, credentialsGindex)) {
            revert InvalidProof("credentials");
        }

        // 7. Verify activation epoch proof
        bytes32 activationEpochLeaf = SSZMerkleVerifier.toLittleEndian64(activationEpoch);
        uint256 activationGindex = SSZMerkleVerifier.validatorActivationEpochGindex(sourceIndex);
        if (!SSZMerkleVerifier.verifyProof(blockRoot, activationEpochLeaf, proofActivationEpoch, activationGindex)) {
            revert InvalidProof("activationEpoch");
        }

        // 8. Check eligibility: activation epoch must be before maxEpoch
        if (activationEpoch >= maxEpoch) {
            revert NotEligible(activationEpoch, maxEpoch);
        }

        // 9. Validate credential prefix and extract recipient address
        bytes1 prefix = sourceCredentials[0];
        if (prefix != 0x01 && prefix != 0x02) {
            revert InvalidCredentialsPrefix(prefix);
        }
        // Last 20 bytes of credentials contain the address
        address recipient = address(uint160(uint256(sourceCredentials)));

        // 10. Check sufficient balance
        if (address(this).balance < rewardAmount) {
            revert InsufficientBalance(rewardAmount, address(this).balance);
        }

        // 11. Mark as claimed (before transfer to prevent reentrancy)
        rewarded[sourceIndex] = true;

        // 12. Transfer reward
        (bool success,) = recipient.call{value: rewardAmount}("");
        if (!success) {
            revert TransferFailed(recipient, rewardAmount);
        }

        // 13. Emit event
        emit RewardClaimed(sourceIndex, recipient, rewardAmount);
    }

    // =========================================================================
    // Admin Functions
    // =========================================================================

    /**
     * @notice Withdraws funds from the contract
     * @param to The recipient address
     * @param amount The amount to withdraw
     */
    function withdraw(address to, uint256 amount) external onlyOwner {
        if (amount > address(this).balance) {
            revert InsufficientBalance(amount, address(this).balance);
        }
        
        (bool success,) = to.call{value: amount}("");
        if (!success) {
            revert TransferFailed(to, amount);
        }
        
        emit Withdrawn(to, amount);
    }

    /**
     * @notice Receive function to accept xDAI funding
     */
    receive() external payable {}

    // =========================================================================
    // Upgrade Authorization
    // =========================================================================

    /**
     * @notice Authorizes an upgrade (UUPS pattern)
     * @param newImplementation The new implementation address
     */
    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    // =========================================================================
    // Internal Functions
    // =========================================================================

    /**
     * @notice Gets the beacon block root for a given timestamp from EIP-4788 oracle
     * @param timestamp The beacon chain timestamp
     * @return The beacon block root, or bytes32(0) if not found
     */
    function _getBeaconBlockRoot(uint64 timestamp) internal view returns (bytes32) {
        (bool success, bytes memory data) = EIP4788_ORACLE.staticcall(abi.encode(timestamp));
        
        if (!success || data.length != 32) {
            return bytes32(0);
        }
        
        return abi.decode(data, (bytes32));
    }
}
