// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract A {
    uint256 public count;
    address public immutable owner;
    uint8 public immutable decimals;

    constructor(address owner_) {
        owner = owner_;
        decimals = 18;
    }
}
