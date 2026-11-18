// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LoopCtor {
    uint256 public count;

    constructor(uint256 n) {
        for (uint256 i = 0; i < n; i++) {
            count = count + 1;
        }
    }

    function get() external view returns (uint256) {
        return count;
    }
}
