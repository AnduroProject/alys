// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "forge-std/Test.sol";
import {Bridge} from "../src/Bridge.sol";

contract BridgeTest is Test {
    Bridge bridge;

    function setUp() public {
        bridge = new Bridge();
    }

    function test_RevertWhen_CannotBurn() public {
        vm.expectRevert();
        vm.deal(address(this), 1000);
        payable(address(bridge)).transfer(100);
    }

    function test_PegOut() public {
        address payable recipient = payable(vm.addr(1));
        assertEq(recipient.balance, 0);

        uint256 pegInAmount = 100000000;
        vm.deal(recipient, 100000000);
        assertEq(recipient.balance, pegInAmount);

        uint256 pegOutAmount = 100;
        vm.prank(recipient);
        bridge.requestPegOut{value: pegOutAmount}(bytes(""));
        assertEq(recipient.balance, pegInAmount - pegOutAmount);
        assertEq(bridge.BURN_ADDRESS().balance, pegOutAmount);
    }
}
