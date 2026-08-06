// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ControlledDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        return value == 42 ? 1 : 0;
    }

    function fixedArray(uint256[2] calldata values) external pure returns (uint256) {
        return values[0] + values[1];
    }
}
