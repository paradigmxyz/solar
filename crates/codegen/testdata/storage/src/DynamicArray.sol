// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Dynamic array test contract
contract DynamicArray {
    uint256[] public values;

    function push(uint256 value) external {
        values.push(value);
    }

    function pop() external {
        values.pop();
    }

    function length() external view returns (uint256) {
        return values.length;
    }

    function get(uint256 index) external view returns (uint256) {
        return values[index];
    }

    function pushMultiple(uint256 a, uint256 b, uint256 c) external {
        values.push(a);
        values.push(b);
        values.push(c);
    }
}
