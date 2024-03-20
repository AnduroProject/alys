// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../src/MockErc20.sol";

contract TransferToken is Script {
    function run() external {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(privateKey);

        MockErc20 token = MockErc20(vm.envAddress("CONTRACT"));

        address recipient = vm.envAddress("RECIPIENT");
        uint256 amount = vm.envUint("AMOUNT");

        uint256 balanceBefore = token.balanceOf(recipient);
        token.transfer(recipient, amount);
        uint256 balanceAfter = token.balanceOf(recipient);
        assert(balanceAfter == balanceBefore + amount);

        console.log("Recipient has balance %s", balanceAfter);

        vm.stopBroadcast();
    }
}
