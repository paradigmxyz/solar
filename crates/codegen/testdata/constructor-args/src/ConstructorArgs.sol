// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ConstructorArgs {
    uint256 public value;
    address public owner;
    
    constructor(uint256 _value, address _owner) {
        value = _value;
        owner = _owner;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
    
    function getOwner() external view returns (address) {
        return owner;
    }
}
