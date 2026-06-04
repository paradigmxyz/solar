// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Minimal test contract - no external calls
contract Minimal {
    function pureAdd(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
}
