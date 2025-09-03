// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract TestError {
    uint256 public number;
    
    function test() public {
        uint256 x = 1;
        uint256 y = 2;
        uint256 z = x + y // Missing semicolon - this will cause a syntax error
    }
}