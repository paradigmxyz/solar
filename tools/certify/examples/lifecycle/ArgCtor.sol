// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ArgCtor {
    uint256 public x;
    address public owner;

    constructor(uint256 x0, address owner0) {
        x = x0;
        owner = owner0;
    }

    function get() external view returns (uint256) {
        return x;
    }
}
