// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test-only oracle mutant: the public product always compiles the same source
// with both compilers. This fixture proves that the symbolic/replay pipeline
// discovers and confirms a cold, input-dependent raw-return mismatch.
contract ControlledDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        return value == 42 ? 2 : 0;
    }

    function fixedArray(uint256[2] calldata values) external pure returns (uint256) {
        return values[0] + values[1];
    }
}
