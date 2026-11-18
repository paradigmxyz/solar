// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LoopInitializer {
    uint256 public total = sum(4);
    uint256 public limit;

    constructor(uint256 n) {
        limit = n;
    }

    function sum(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s = s + i;
        }
    }

    function get() external view returns (uint256) {
        return total;
    }
}
