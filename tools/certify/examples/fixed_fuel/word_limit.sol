// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A counted loop to an argument word: a call can ask for 2^256 - 1 iterations.
contract SumTo {
    uint256 total;

    function run(uint256 n) external {
        uint256 s = total;
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
        total = s;
    }

    function get() external view returns (uint256) {
        return total;
    }
}
