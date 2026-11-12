// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Masks {
    uint256 x;
    function inc(uint256 d) external {
        x += d;
    }
    function get() external view returns (uint256) {
        return x;
    }
    function low(uint8 a) external pure returns (uint8) {
        return a & 0x0f;
    }
}
