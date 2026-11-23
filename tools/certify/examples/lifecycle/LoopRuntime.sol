// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LoopRuntime {
    uint256 public count;
    uint256 public limit;

    constructor(uint256 n) {
        limit = n;
    }

    function bump(uint256 k) external {
        for (uint256 i = 0; i < k; i++) {
            require(count + 1 <= limit, "over");
            count = count + 1;
        }
    }
}
