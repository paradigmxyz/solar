// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

struct TupleInputInner {
    uint256 first;
    uint256 second;
}

struct TupleInputOuter {
    address owner;
    TupleInputInner inner;
}

// Test-only oracle mutant for nested tuple calldata and independent replay.
contract TupleInputDifferential {
    function probe(TupleInputOuter calldata value) external pure returns (uint256) {
        if (
            value.owner == address(0x1234)
                && value.inner.first == 42
                && value.inner.second == 99
        ) {
            return 2;
        }
        return 0;
    }
}
