// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract JumpThread {
    uint256 s;
    function f(uint256 x, uint256 v) external {
        if (x < 10) {
            if (v > 3) { s = v; }
        }
    }
}
