// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test-only oracle mutant. The public product always compiles one source with
// both compilers.
contract CalldataContextDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        if (msg.sig != bytes4(keccak256("probe(uint256)")) || msg.data.length != 36) {
            return 100;
        }
        return value == 42 ? 2 : 0;
    }
}
