// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract LoadPre {
    uint256 s;
    function f(uint256 x, uint256 v) external returns (uint256) {
        if (x < 10) { s = v; }
        return s;
    }
}
