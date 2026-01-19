// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Events - Contract with events for equivalence testing
contract Events {
    event Transfer(address indexed from, address indexed to, uint256 value);
    event ValueSet(uint256 indexed id, uint256 value);

    uint256 public lastValue;

    function emitTransfer(address from, address to, uint256 value) external {
        emit Transfer(from, to, value);
    }

    function setValue(uint256 id, uint256 value) external {
        lastValue = value;
        emit ValueSet(id, value);
    }
}
