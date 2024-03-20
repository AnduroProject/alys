// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../src/Bridge.sol";

contract RequestPegOut is Script {
    function run() external {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(privateKey);

        Bridge bridge = Bridge(payable(0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB));

        uint256 satoshis = vm.envUint("SATOSHIS");
        uint256 amount = satoshis * 10 ** 10;

        console.log("Balance: %s", vm.addr(privateKey).balance);
        console.log("Burning: %s", amount);

        bridge.requestPegOut{value: amount}(bytes(vm.envString("BTC_ADDRESS")));

        vm.stopBroadcast();
    }
}
