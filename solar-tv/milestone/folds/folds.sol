// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Folds {
    function g(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 k = 10;
        uint256 t = x;
        if (k > 5) {
            t = y - x;
        }
        return t;
    }
}
