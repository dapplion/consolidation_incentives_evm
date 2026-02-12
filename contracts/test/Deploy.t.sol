// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Test, console} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ConsolidationIncentives} from "../src/ConsolidationIncentives.sol";

/// @title Deployment Test
/// @notice Tests the deployment script logic
contract DeployTest is Test {
    ConsolidationIncentives public implementation;
    ERC1967Proxy public proxy;
    ConsolidationIncentives public instance;

    address public deployer = makeAddr("deployer");
    uint64 public constant MAX_EPOCH = 10000;
    uint256 public constant REWARD_AMOUNT = 1 ether;
    uint256 public constant MIN_CLAIM_DELAY = 960;

    function setUp() public {
        vm.deal(deployer, 100 ether);
    }

    function test_deployment_withoutFunding() public {
        vm.startPrank(deployer);

        // Deploy implementation
        implementation = new ConsolidationIncentives();
        
        // Encode initializer
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        
        // Deploy proxy
        proxy = new ERC1967Proxy(address(implementation), initData);
        instance = ConsolidationIncentives(payable(address(proxy)));

        vm.stopPrank();

        // Verify deployment
        assertEq(instance.owner(), deployer, "Owner should be deployer");
        assertEq(instance.maxEpoch(), MAX_EPOCH, "Max epoch should match");
        assertEq(instance.rewardAmount(), REWARD_AMOUNT, "Reward amount should match");
        assertEq(instance.minClaimDelay(), MIN_CLAIM_DELAY, "Min claim delay should match");
        assertEq(address(proxy).balance, 0, "Contract should have no balance");
    }

    function test_deployment_withFunding() public {
        uint256 initialFunding = 50 ether;

        vm.startPrank(deployer);

        // Deploy implementation
        implementation = new ConsolidationIncentives();
        
        // Encode initializer
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        
        // Deploy proxy
        proxy = new ERC1967Proxy(address(implementation), initData);
        
        // Fund contract
        (bool success,) = address(proxy).call{value: initialFunding}("");
        require(success, "Funding failed");

        instance = ConsolidationIncentives(payable(address(proxy)));

        vm.stopPrank();

        // Verify funding
        assertEq(address(proxy).balance, initialFunding, "Contract should be funded");
    }

    function test_deployment_cannotReinitialize() public {
        vm.startPrank(deployer);

        // Deploy
        implementation = new ConsolidationIncentives();
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        proxy = new ERC1967Proxy(address(implementation), initData);
        instance = ConsolidationIncentives(payable(address(proxy)));

        // Try to reinitialize - should revert
        vm.expectRevert();
        instance.initialize(deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY);

        vm.stopPrank();
    }

    function test_deployment_upgradeability() public {
        vm.startPrank(deployer);

        // Deploy initial version
        implementation = new ConsolidationIncentives();
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        proxy = new ERC1967Proxy(address(implementation), initData);
        instance = ConsolidationIncentives(payable(address(proxy)));

        // Deploy new implementation
        ConsolidationIncentives newImplementation = new ConsolidationIncentives();

        // Upgrade
        instance.upgradeToAndCall(address(newImplementation), "");

        // Verify state persists
        assertEq(instance.maxEpoch(), MAX_EPOCH, "State should persist after upgrade");
        assertEq(instance.owner(), deployer, "Owner should persist after upgrade");

        vm.stopPrank();
    }

    function test_deployment_onlyOwnerCanUpgrade() public {
        vm.startPrank(deployer);

        // Deploy
        implementation = new ConsolidationIncentives();
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        proxy = new ERC1967Proxy(address(implementation), initData);
        instance = ConsolidationIncentives(payable(address(proxy)));

        vm.stopPrank();

        // Non-owner tries to upgrade
        address attacker = makeAddr("attacker");
        vm.startPrank(attacker);

        ConsolidationIncentives newImplementation = new ConsolidationIncentives();
        
        vm.expectRevert();
        instance.upgradeToAndCall(address(newImplementation), "");

        vm.stopPrank();
    }

    function test_deployment_proxyPointsToImplementation() public {
        vm.startPrank(deployer);

        implementation = new ConsolidationIncentives();
        bytes memory initData = abi.encodeCall(
            ConsolidationIncentives.initialize,
            (deployer, MAX_EPOCH, REWARD_AMOUNT, MIN_CLAIM_DELAY)
        );
        proxy = new ERC1967Proxy(address(implementation), initData);

        // Read implementation slot (ERC1967 standard slot)
        bytes32 implSlot = vm.load(
            address(proxy),
            bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1)
        );
        address storedImpl = address(uint160(uint256(implSlot)));

        assertEq(storedImpl, address(implementation), "Proxy should point to implementation");

        vm.stopPrank();
    }
}
