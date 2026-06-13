// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Counter - Simple counter contract for bytecode equivalence testing
contract Counter {
    uint256 public count;

    function increment() public {
        count = count + 1;
    }

    function getCount() public view returns (uint256) {
        return count;
    }
}
