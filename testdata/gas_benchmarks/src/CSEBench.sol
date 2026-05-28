// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Common Subexpression Elimination Benchmark
/// @notice Tests CSE optimization opportunities
contract CSEBench {
    uint256 public value;

    /// @notice Same expression computed multiple times
    /// CSE should cache (a + b) and reuse it
    function duplicateAdd(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 sum1 = a + b;
        uint256 sum2 = a + b;  // Should reuse sum1
        return sum1 + sum2;
    }

    /// @notice Commutative CSE test
    /// CSE should recognize a + b == b + a
    function commutativeAdd(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 sum1 = a + b;
        uint256 sum2 = b + a;  // Should reuse sum1 (commutative)
        return sum1 * sum2;
    }

    /// @notice Complex expression reuse
    function complexCSE(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        uint256 x = (a * b) + c;
        uint256 y = (a * b) - c;  // (a * b) should be cached
        return x + y;  // Simplifies to 2 * (a * b)
    }

    /// @notice Storage read CSE
    /// Multiple reads from same slot should be cached
    function storageCSE() public view returns (uint256) {
        uint256 v1 = value;
        uint256 v2 = value;  // Should reuse v1
        uint256 v3 = value;  // Should reuse v1
        return v1 + v2 + v3;
    }

    /// @notice Non-CSE candidate (different operands)
    function nonCSE(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        uint256 x = a + b;
        uint256 y = b + c;  // Different operands, can't reuse
        return x + y;
    }

    /// @notice Multiplication CSE with commutative recognition
    function mulCSE(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 prod1 = a * b;
        uint256 prod2 = b * a;  // Should reuse prod1
        return prod1 + prod2;
    }

    /// @notice Bitwise CSE
    function bitwiseCSE(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 x = a & b;
        uint256 y = a & b;  // Should reuse x
        uint256 z = a | b;
        uint256 w = b | a;  // Should reuse z (commutative)
        return x + y + z + w;
    }

    /// @notice Comparison CSE
    function comparisonCSE(uint256 a, uint256 b) public pure returns (bool) {
        bool eq1 = a == b;
        bool eq2 = b == a;  // Should reuse eq1 (commutative)
        return eq1 && eq2;
    }
}
