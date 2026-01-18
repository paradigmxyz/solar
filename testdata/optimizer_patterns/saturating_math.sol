// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for saturating arithmetic optimizations
/// @dev Solar should recognize these patterns and emit branchless code

contract SaturatingMath {
    /// @dev Zero-floor subtraction: max(0, x - y)
    /// Optimal: z := mul(gt(x, y), sub(x, y))
    function zeroFloorSub(uint256 x, uint256 y) public pure returns (uint256) {
        return x > y ? x - y : 0;
    }

    /// @dev Alternative spelling of zero-floor sub
    function saturatingSub(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            return x >= y ? x - y : 0;
        }
    }

    /// @dev Saturating add: min(2^256 - 1, x + y)
    /// Optimal: z := or(sub(0, lt(add(x, y), x)), add(x, y))
    function saturatingAdd(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            uint256 z = x + y;
            return z >= x ? z : type(uint256).max;
        }
    }

    /// @dev Alternative: return max if overflow detected
    function saturatingAddAlt(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            uint256 z = x + y;
            if (z < x) return type(uint256).max;
            return z;
        }
    }

    /// @dev Saturating mul: min(2^256 - 1, x * y)
    /// Optimal: z := or(sub(or(iszero(x), eq(div(mul(x, y), x), y)), 1), mul(x, y))
    function saturatingMul(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            if (x == 0) return 0;
            uint256 z = x * y;
            return z / x == y ? z : type(uint256).max;
        }
    }

    /// @dev Bounded increment: min(max, x + 1)
    function boundedIncrement(uint256 x, uint256 maxVal) public pure returns (uint256) {
        return x < maxVal ? x + 1 : maxVal;
    }

    /// @dev Bounded decrement: max(min, x - 1)
    function boundedDecrement(uint256 x, uint256 minVal) public pure returns (uint256) {
        return x > minVal ? x - 1 : minVal;
    }

    /// @dev Clamp to range
    function clamp(uint256 x, uint256 minVal, uint256 maxVal) public pure returns (uint256) {
        if (x < minVal) return minVal;
        if (x > maxVal) return maxVal;
        return x;
    }

    /// @dev Average without overflow
    /// Optimal: z := add(and(x, y), shr(1, xor(x, y)))
    function average(uint256 x, uint256 y) public pure returns (uint256) {
        unchecked {
            return (x & y) + ((x ^ y) >> 1);
        }
    }

    /// @dev Naive average that overflows
    function averageNaive(uint256 x, uint256 y) public pure returns (uint256) {
        return (x + y) / 2;
    }
}
