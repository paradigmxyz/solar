// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Events {
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event SimpleEvent(uint256 value);

    function emitSimple(uint256 val) public {
        emit SimpleEvent(val);
    }

    function emitTransfer(address from, address to, uint256 value) public {
        emit Transfer(from, to, value);
    }
}
