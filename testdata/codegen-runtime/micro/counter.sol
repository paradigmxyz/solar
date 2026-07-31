
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;

    function increment(uint256 times) external {
        for (uint256 i = 0; i < times; ++i) {
            count += 1;
        }
    }

    function reset() external {
        count = 0;
    }
}
