// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LoopCtorHelper {
    uint256 public count;

    constructor(uint256 n) {
        fill(n);
    }

    function fill(uint256 n) internal {
        for (uint256 i = 0; i < n; i++) {
            count = count + 1;
        }
    }

    function get() external view returns (uint256) {
        return count;
    }
}
