// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StorageInit {
    uint256 public value = 42;
    uint256 public anotherValue = 100;
    address public owner = address(0x1234);

    function getValue() public view returns (uint256) {
        return value;
    }

    function getAnotherValue() public view returns (uint256) {
        return anotherValue;
    }
}
