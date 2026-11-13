// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Loops {
    function sum(uint256[] calldata xs) external pure returns (uint256 s) {
        for (uint256 i = 0; i < xs.length; i++) {
            s += xs[i];
        }
    }

    function halve(uint256 x) external pure returns (uint256 n) {
        while (x > 1) {
            x >>= 1;
            n++;
        }
    }
}
