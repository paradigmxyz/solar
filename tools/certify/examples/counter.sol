// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Counter {
    uint256 x;
    function inc(uint256 d) external { x += d; }
    function get() external view returns (uint256) { return x; }
}
