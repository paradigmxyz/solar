// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Checks {
    function f(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 p = x + y;
        uint256 q = x + y;
        return (p ^ q) + (x & x) + 7;
    }
}
