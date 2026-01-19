// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Modifiers {
    address public owner;
    uint256 public value;
    bool public locked;
    
    constructor() {
        owner = msg.sender;
    }
    
    modifier onlyOwner() {
        require(msg.sender == owner);
        _;
    }
    
    modifier nonReentrant() {
        require(!locked);
        locked = true;
        _;
        locked = false;
    }
    
    modifier validValue(uint256 v) {
        require(v > 0);
        _;
    }
    
    function setValue(uint256 v) external onlyOwner validValue(v) {
        value = v;
    }
    
    function setValueNonReentrant(uint256 v) external nonReentrant {
        value = v;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
}
