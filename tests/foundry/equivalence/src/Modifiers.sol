// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Modifiers - Contract with modifiers for equivalence testing
contract Modifiers {
    address public owner;
    uint256 public value;
    bool public paused;

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    modifier whenNotPaused() {
        require(!paused, "paused");
        _;
    }

    function setValue(uint256 v) external onlyOwner whenNotPaused {
        value = v;
    }

    function setPaused(bool p) external onlyOwner {
        paused = p;
    }

    function transferOwnership(address newOwner) external onlyOwner {
        owner = newOwner;
    }
}
