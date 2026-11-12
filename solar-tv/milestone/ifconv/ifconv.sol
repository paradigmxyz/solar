// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract IfConv {
    function pick(uint256 x, uint256 a, uint256 b) external pure returns (uint256) {
        uint256 r;
        if (x < 10) { r = a ^ 3; } else { r = b | 5; }
        return r;
    }
}
