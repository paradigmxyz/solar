
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {
    uint256 public value;

    function compute(uint256 a, uint256 b, uint256 iterations) external {
        value = a;
        for (uint256 i = 0; i < iterations; ++i) {
            value = (value * b + a) / 2;
            value = value % 1000000 + 1;
        }
    }
}
