// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Arithmetic {
    uint256 private stored;

    constructor() {
        stored = 7; // debug-check: constructor
    }

    function increment(uint256 value) external pure returns (uint256) {
        return value + 1; // debug-check: increment
    }

    function branch(uint256 value) external pure returns (uint256) {
        if (value > 10) {
            return value - 1; // debug-check: branch-then
        }
        return value + 2; // debug-check: branch-else
    }

    function read() external view returns (uint256) {
        return stored; // debug-check: read
    }

    function checked(uint256 value) external pure returns (uint256) {
        require(value != 0, "zero"); // debug-check: require
        return value; // debug-check: checked
    }
}
