// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AggregateReturns {
    function tuple(uint256 value) external pure returns (uint256, uint256) {
        return pair(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value + 1, value + 2); // debug-check: tuple
    }
}
