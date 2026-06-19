// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Nested Struct Memory Tests
/// @notice Tests nested struct access in memory (no storage copying)

struct InnerPair {
    uint256 a;
    uint256 b;
}

struct OuterTriple {
    InnerPair pair;
    uint256 c;
}

contract NestedMemory {
    /// @notice Create nested struct in memory and access fields
    function nestedSum() public pure returns (uint256) {
        OuterTriple memory o;
        o.pair.a = 1;
        o.pair.b = 2;
        o.c = 3;
        return o.pair.a + o.pair.b + o.c;
    }

    /// @notice Nested struct with different values
    function nestedValues() public pure returns (uint256 a, uint256 b, uint256 c) {
        OuterTriple memory o;
        o.pair.a = 100;
        o.pair.b = 200;
        o.c = 300;
        return (o.pair.a, o.pair.b, o.c);
    }

    /// @notice Multiple nested structs in memory
    function multipleNested() public pure returns (uint256) {
        OuterTriple memory x;
        OuterTriple memory y;
        x.pair.a = 1;
        x.pair.b = 2;
        x.c = 3;
        y.pair.a = 10;
        y.pair.b = 20;
        y.c = 30;
        return x.pair.a + x.pair.b + x.c + y.pair.a + y.pair.b + y.c;
    }
}
