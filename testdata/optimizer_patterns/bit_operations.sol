// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for bit operation optimizations
/// @dev Solar should recognize log2, popcount, and other bit patterns

contract BitOperations {
    /// @dev Log2 floor - count most significant bit position
    /// Optimal: Binary search pattern from Solady
    function log2Floor(uint256 x) public pure returns (uint256 r) {
        require(x > 0, "log2(0)");
        
        assembly {
            // Binary search approach
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, shl(2, lt(0xf, shr(r, x))))
            r := or(r, shl(1, lt(0x3, shr(r, x))))
            r := or(r, lt(0x1, shr(r, x)))
        }
    }

    /// @dev Log2 ceiling
    function log2Ceil(uint256 x) public pure returns (uint256) {
        require(x > 0, "log2(0)");
        uint256 floor = log2Floor(x);
        return floor + (x > (1 << floor) ? 1 : 0);
    }

    /// @dev Check if power of 2
    /// Optimal: iszero(and(x, sub(x, 1)))
    function isPowerOfTwo(uint256 x) public pure returns (bool) {
        return x != 0 && (x & (x - 1)) == 0;
    }

    /// @dev Next power of 2
    function nextPowerOfTwo(uint256 x) public pure returns (uint256) {
        if (x == 0) return 1;
        if (isPowerOfTwo(x)) return x;
        return 1 << (log2Floor(x) + 1);
    }

    /// @dev Count leading zeros
    function clz(uint256 x) public pure returns (uint256) {
        if (x == 0) return 256;
        return 255 - log2Floor(x);
    }

    /// @dev Count trailing zeros
    function ctz(uint256 x) public pure returns (uint256 r) {
        if (x == 0) return 256;
        
        assembly {
            // Isolate lowest bit: x & (-x)
            x := and(x, sub(0, x))
            // Then log2
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, shl(2, lt(0xf, shr(r, x))))
            r := or(r, shl(1, lt(0x3, shr(r, x))))
            r := or(r, lt(0x1, shr(r, x)))
        }
    }

    /// @dev Population count (hamming weight)
    function popcount(uint256 x) public pure returns (uint256 count) {
        // Parallel bit count
        unchecked {
            x = x - ((x >> 1) & 0x5555555555555555555555555555555555555555555555555555555555555555);
            x = (x & 0x3333333333333333333333333333333333333333333333333333333333333333) 
              + ((x >> 2) & 0x3333333333333333333333333333333333333333333333333333333333333333);
            x = (x + (x >> 4)) & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f;
            x = x + (x >> 8);
            x = x + (x >> 16);
            x = x + (x >> 32);
            x = x + (x >> 64);
            x = x + (x >> 128);
            count = x & 0xff;
        }
    }

    /// @dev Reverse bits
    function reverseBits(uint256 x) public pure returns (uint256) {
        unchecked {
            x = ((x >> 1) & 0x5555555555555555555555555555555555555555555555555555555555555555)
              | ((x & 0x5555555555555555555555555555555555555555555555555555555555555555) << 1);
            x = ((x >> 2) & 0x3333333333333333333333333333333333333333333333333333333333333333)
              | ((x & 0x3333333333333333333333333333333333333333333333333333333333333333) << 2);
            x = ((x >> 4) & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f)
              | ((x & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f) << 4);
            x = ((x >> 8) & 0x00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff)
              | ((x & 0x00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff) << 8);
            // Continue for 16, 32, 64, 128
        }
        return x;
    }

    /// @dev Get bit at position
    function getBit(uint256 x, uint8 pos) public pure returns (bool) {
        return (x >> pos) & 1 == 1;
    }

    /// @dev Set bit at position
    function setBit(uint256 x, uint8 pos) public pure returns (uint256) {
        return x | (1 << pos);
    }

    /// @dev Clear bit at position
    function clearBit(uint256 x, uint8 pos) public pure returns (uint256) {
        return x & ~(1 << pos);
    }

    /// @dev Toggle bit at position
    function toggleBit(uint256 x, uint8 pos) public pure returns (uint256) {
        return x ^ (1 << pos);
    }

    /// @dev Max uint256 via not(0)
    /// Optimal: not(0) instead of push32 0xfff...fff
    function maxUint() public pure returns (uint256) {
        return type(uint256).max;
    }

    /// @dev 32-byte alignment mask
    function align32(uint256 x) public pure returns (uint256) {
        return x & ~uint256(0x1f);
    }
}
