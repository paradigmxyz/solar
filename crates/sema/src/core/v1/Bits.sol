// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Bit counting.
/// @dev Compiler-owned module, imported as `solar:core/v1/Bits.sol`. The
/// bodies are plain Solidity; where the target EVM has a matching instruction
/// the compiler uses it instead, and otherwise the body is what runs.
library Bits {
    /// @dev The number of zero bits above the highest set bit of `x`; 256
    /// for zero.
    function leadingZeros(uint256 x) internal pure returns (uint256 n) {
        if (x == 0) return 256;
        if (x >> 128 == 0) {
            n += 128;
            x <<= 128;
        }
        if (x >> 192 == 0) {
            n += 64;
            x <<= 64;
        }
        if (x >> 224 == 0) {
            n += 32;
            x <<= 32;
        }
        if (x >> 240 == 0) {
            n += 16;
            x <<= 16;
        }
        if (x >> 248 == 0) {
            n += 8;
            x <<= 8;
        }
        if (x >> 252 == 0) {
            n += 4;
            x <<= 4;
        }
        if (x >> 254 == 0) {
            n += 2;
            x <<= 2;
        }
        if (x >> 255 == 0) n += 1;
    }

    /// @dev The number of zero bits below the lowest set bit of `x`; 256 for
    /// zero.
    function trailingZeros(uint256 x) internal pure returns (uint256) {
        if (x == 0) return 256;
        // Isolating the lowest set bit leaves a power of two, whose position
        // is what the leading count of it reports from the other end.
        return 255 - leadingZeros(x & (~x + 1));
    }

    /// @dev The number of set bits in `x`.
    function popCount(uint256 x) internal pure returns (uint256) {
        // Sum bits in ever wider lanes: pairs, nibbles, bytes, then all bytes
        // at once through a multiplication that stacks them into the top one.
        // No lane can carry into its neighbour; the final product is meant to
        // wrap, only its top byte is the answer. That byte holds at most 255,
        // so the lowest bit is counted apart and the lanes see 255 bits.
        unchecked {
            uint256 lowest = x & 1;
            x >>= 1;
            x = x - ((x >> 1) & 0x5555555555555555555555555555555555555555555555555555555555555555);
            x = (x & 0x3333333333333333333333333333333333333333333333333333333333333333)
                + ((x >> 2) & 0x3333333333333333333333333333333333333333333333333333333333333333);
            x = (x + (x >> 4)) & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f;
            return ((x * 0x0101010101010101010101010101010101010101010101010101010101010101) >> 248)
                + lowest;
        }
    }
}
