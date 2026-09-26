// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Child {
    struct Config {
        uint256 number;
        string name;
    }
    uint256 public number;
    string public name;
    address public creator;
    uint256 public paid;

    error InvalidNumber(uint256 number);

    constructor(Config memory config) payable {
        if (config.number == 0) revert InvalidNumber(config.number);
        number = config.number;
        name = config.name;
        creator = msg.sender;
        paid = msg.value;
    }
}

contract Parent {
    Child public child;

    constructor(Child child_) {
        child = child_;
    }
}
