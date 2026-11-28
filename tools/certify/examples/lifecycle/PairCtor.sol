// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract PairCtor {
    uint256 public a;
    uint256 public b;

    constructor(uint256 a0, uint256 b0) {
        a = a0;
        b = a0 + b0;
    }

    function get() external view returns (uint256) {
        return b;
    }
}
