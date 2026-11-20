// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A counted loop with a literal limit: at most 10 iterations on every call.
contract SumTen {
    uint256 total;

    function run() external {
        uint256 s = total;
        for (uint256 i = 0; i < 10; i++) {
            s += i;
        }
        total = s;
    }

    function get() external view returns (uint256) {
        return total;
    }
}
