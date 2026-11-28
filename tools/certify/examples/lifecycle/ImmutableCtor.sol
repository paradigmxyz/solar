// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ImmutableCtor {
    uint256 public immutable limit;
    uint256 public y;

    constructor(uint256 limit0) {
        limit = limit0;
        y = limit0 + 1;
    }

    function get() external view returns (uint256) {
        return limit;
    }
}
