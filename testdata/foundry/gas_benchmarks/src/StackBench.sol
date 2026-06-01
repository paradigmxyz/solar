// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stack Operations Benchmark
/// @notice Tests stack scheduling and DUP/SWAP optimization
contract StackBench {
    /// @notice Value reuse - should minimize DUPs
    function valueReuse(uint256 x) public pure returns (uint256) {
        // x is used 4 times - optimal scheduling minimizes DUPs
        return x + x + x + x;
    }

    /// @notice Deep stack access pattern
    function deepStack(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h
    ) public pure returns (uint256) {
        // Access pattern that tests stack depth management
        return a + h + b + g + c + f + d + e;
    }

    /// @notice Expression that creates temporary values
    function tempValues(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        // Optimizer should eliminate dead temporaries
        uint256 t1 = a + b;
        uint256 t2 = b + c;
        uint256 t3 = a + c;
        // Only t1 and t2 used in final result
        return t1 * t2;
    }

    /// @notice Nested expressions requiring careful ordering
    function nestedExpr(uint256 a, uint256 b, uint256 c, uint256 d) public pure returns (uint256) {
        return ((a + b) * (c + d)) + ((a - b) * (c - d));
    }

    /// @notice Multiple return values requiring stack management
    function multiReturn(uint256 x, uint256 y) public pure returns (uint256, uint256, uint256) {
        uint256 sum = x + y;
        uint256 diff = x - y;
        uint256 prod = x * y;
        return (sum, diff, prod);
    }

    /// @notice Function with many local variables
    function manyLocals(uint256 input) public pure returns (uint256) {
        uint256 v1 = input + 1;
        uint256 v2 = v1 + 1;
        uint256 v3 = v2 + 1;
        uint256 v4 = v3 + 1;
        uint256 v5 = v4 + 1;
        uint256 v6 = v5 + 1;
        uint256 v7 = v6 + 1;
        uint256 v8 = v7 + 1;
        // Final expression uses multiple locals
        return v1 + v4 + v8;
    }

    /// @notice Pattern that might cause stack-too-deep in naive compilation
    function complexStack(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h,
        uint256 i, uint256 j
    ) public pure returns (uint256) {
        uint256 r1 = a + b + c;
        uint256 r2 = d + e + f;
        uint256 r3 = g + h + i + j;
        return r1 + r2 + r3;
    }
}
