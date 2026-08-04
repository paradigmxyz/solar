
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SumStorage {
    uint256 public total;

    function sumRange(uint256 start, uint256 end) external {
        total = 0;
        for (uint256 i = start; i <= end; ++i) {
            total += i;
        }
    }
}
