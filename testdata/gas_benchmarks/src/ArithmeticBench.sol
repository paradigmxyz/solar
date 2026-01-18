// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Arithmetic Optimization Benchmark
/// @notice Tests constant folding and identity optimizations
contract ArithmeticBench {
    /// @notice Add zero - should be eliminated
    function addZero(uint256 x) public pure returns (uint256) {
        return x + 0;
    }

    /// @notice Subtract zero - should be eliminated
    function subZero(uint256 x) public pure returns (uint256) {
        return x - 0;
    }

    /// @notice Multiply by one - should be eliminated
    function mulOne(uint256 x) public pure returns (uint256) {
        return x * 1;
    }

    /// @notice Divide by one - should be eliminated
    function divOne(uint256 x) public pure returns (uint256) {
        return x / 1;
    }

    /// @notice Multiply by zero - should fold to zero
    function mulZero(uint256 x) public pure returns (uint256) {
        return x * 0;
    }

    /// @notice Constant expression - should be folded at compile time
    function constExpr() public pure returns (uint256) {
        return 10 + 20 + 30;  // Should fold to 60
    }

    /// @notice Complex constant expression
    function complexConstExpr() public pure returns (uint256) {
        return (5 * 10) + (3 * 4) - 2;  // Should fold to 60
    }

    /// @notice OR with zero - should be eliminated
    function orZero(uint256 x) public pure returns (uint256) {
        return x | 0;
    }

    /// @notice AND with all ones - should be eliminated
    function andAllOnes(uint256 x) public pure returns (uint256) {
        return x & type(uint256).max;
    }

    /// @notice XOR with zero - should be eliminated
    function xorZero(uint256 x) public pure returns (uint256) {
        return x ^ 0;
    }

    /// @notice Shift by zero - should be eliminated
    function shlZero(uint256 x) public pure returns (uint256) {
        return x << 0;
    }

    /// @notice Shift by zero - should be eliminated
    function shrZero(uint256 x) public pure returns (uint256) {
        return x >> 0;
    }

    /// @notice Power of 2 multiplication optimization
    function mulPow2(uint256 x) public pure returns (uint256) {
        return x * 8;  // Could become x << 3
    }

    /// @notice Power of 2 division optimization
    function divPow2(uint256 x) public pure returns (uint256) {
        return x / 4;  // Could become x >> 2
    }

    /// @notice Double negation - should cancel out
    function doubleNot(uint256 x) public pure returns (uint256) {
        return ~~x;
    }

    /// @notice Chained operations with identities
    function chainedIdentities(uint256 x) public pure returns (uint256) {
        uint256 a = x + 0;
        uint256 b = a * 1;
        uint256 c = b - 0;
        uint256 d = c / 1;
        return d;  // Should simplify to just x
    }

    /// @notice Mixed constants and variables
    function mixedExpr(uint256 x, uint256 y) public pure returns (uint256) {
        uint256 constPart = 10 + 20;  // Should fold to 30
        return x + constPart + y;
    }
}
