// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract NoArgCtor {
    uint256 public x;

    constructor() {
        x = 7;
    }

    function get() external view returns (uint256) {
        return x;
    }
}
