// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Initializers {
    uint256 stored = 42;
    uint8 packed = 7;
    string text = "initial";
    uint256 immutable limit = 100;
    uint8 immutable narrow = 5;

    function read() external view returns (uint256, uint8, string memory, uint256, uint8) {
        return (stored, packed, text, limit, narrow);
    }
}

contract Base {
    string public label;

    constructor(string memory label_) {
        label = label_;
    }
}

contract Derived is Base("base label") {}
