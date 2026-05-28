// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Three Level Mapping - Minimal test case
contract ThreeLevelMapping {
    mapping(uint256 => mapping(uint256 => mapping(uint256 => uint256))) public data;
    
    function set(uint256 a, uint256 b, uint256 c, uint256 val) public {
        data[a][b][c] = val;
    }
    
    function get(uint256 a, uint256 b, uint256 c) public view returns (uint256) {
        return data[a][b][c];
    }
}
