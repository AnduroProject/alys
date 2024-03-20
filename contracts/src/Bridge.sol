// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

contract Bridge {
    address payable public constant BURN_ADDRESS = payable(0x000000000000000000000000000000000000dEaD);

    // NOTE: topics are fixed sizes, so we can't index the bitcoinAddress
    event RequestPegOut(address indexed _evmAddress, bytes _bitcoinAddress, uint256 _value);

    // Calling this function burns the wrapped BTC, the federation
    // should then receive the event and transfer the BTC on Bitcoin
    // NOTE: ensure the `_bitcoinAddress` is valid before calling
    function requestPegOut(bytes calldata _bitcoinAddress) public payable {
        // TODO: check dust amount
        require(msg.value >= 0, "Insufficient amount");

        BURN_ADDRESS.transfer(msg.value);

        emit RequestPegOut(msg.sender, _bitcoinAddress, msg.value);
    }

    // NOTE: not required, contract should throw if not defined
    receive() external payable {
        revert("Cannot burn without BTC address");
    }

    fallback() external payable {}
}
