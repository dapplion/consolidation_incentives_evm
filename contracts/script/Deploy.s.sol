// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Script, console} from "forge-std/Script.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ConsolidationIncentives} from "../src/ConsolidationIncentives.sol";

/// @title Deploy Script for ConsolidationIncentives
/// @notice Deploys the UUPS upgradeable contract with ERC1967Proxy
/// @dev Usage:
///   Dry run: forge script script/Deploy.s.sol --rpc-url <RPC>
///   Deploy:  forge script script/Deploy.s.sol --rpc-url <RPC> --broadcast --verify
///
/// Environment variables:
///   MAX_EPOCH           - Eligibility cutoff epoch (required)
///   REWARD_AMOUNT       - Reward amount in wei (required)
///   MIN_CLAIM_DELAY     - Minimum claim delay in seconds (default: 960 = 12 epochs on Gnosis)
///   INITIAL_FUNDING     - Initial contract funding in wei (optional, default: 0)
contract DeployScript is Script {
    /// @notice Deploy the contract
    function run() external {
        // Read configuration from environment
        uint64 maxEpoch = uint64(vm.envUint("MAX_EPOCH"));
        uint256 rewardAmount = vm.envUint("REWARD_AMOUNT");
        uint256 minClaimDelay = vm.envOr("MIN_CLAIM_DELAY", uint256(960)); // 12 epochs * 80s
        uint256 initialFunding = vm.envOr("INITIAL_FUNDING", uint256(0));

        require(maxEpoch > 0, "MAX_EPOCH must be set and > 0");
        require(rewardAmount > 0, "REWARD_AMOUNT must be set and > 0");

        console.log("=== ConsolidationIncentives Deployment ===");
        console.log("Max Epoch:", maxEpoch);
        console.log("Reward Amount (wei):", rewardAmount);
        console.log("Reward Amount (xDAI):", rewardAmount / 1e18);
        console.log("Min Claim Delay (s):", minClaimDelay);
        console.log("Initial Funding (wei):", initialFunding);
        console.log("Deployer:", msg.sender);

        vm.startBroadcast();

        // 1. Deploy implementation
        ConsolidationIncentives implementation = new ConsolidationIncentives();
        console.log("Implementation deployed at:", address(implementation));

        // 2. Encode initializer call
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (msg.sender, maxEpoch, rewardAmount, minClaimDelay)
        );

        // 3. Deploy proxy
        ERC1967Proxy proxy = new ERC1967Proxy(address(implementation), initData);
        console.log("Proxy deployed at:", address(proxy));

        // 4. Fund contract if requested
        if (initialFunding > 0) {
            (bool success,) = address(proxy).call{value: initialFunding}("");
            require(success, "Funding transfer failed");
            console.log("Contract funded with:", initialFunding);
        }

        vm.stopBroadcast();

        // 5. Verify deployment
        ConsolidationIncentives instance = ConsolidationIncentives(payable(address(proxy)));
        console.log("\n=== Deployment Verification ===");
        console.log("Owner:", instance.owner());
        console.log("Max Epoch:", instance.maxEpoch());
        console.log("Reward Amount:", instance.rewardAmount());
        console.log("Min Claim Delay:", instance.minClaimDelay());
        console.log("Contract Balance:", address(proxy).balance);

        console.log("\n=== Summary ===");
        console.log("Proxy Address (use this):", address(proxy));
        console.log("Implementation Address:", address(implementation));
    }
}
