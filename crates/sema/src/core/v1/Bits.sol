// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Bit scanning and counting.
/// @dev Compiler-owned module, imported as `solar:core/v1/Bits.sol`. Every
/// function is total: a zero input gives 256, which no bit index or count of
/// a non-zero word can be. The bodies are plain Solidity; where the target EVM
/// has a matching instruction the compiler uses it instead, and otherwise the
/// body is what runs.
library Bits {
    /// @dev Selects a table slot from a byte value: shifted right by the byte
    /// and masked to five bits, it gives a distinct slot for each position the
    /// byte's highest set bit can have, and slot 30 for a zero byte.
    uint256 private constant _SELECTOR = 0x8421084210842108cc6318c6db6d54be;

    /// @dev The highest set bit's index within a byte, per selector slot.
    bytes32 private constant _HIGHEST =
        0x0706060506020504060203020504030106050205030304010505030400000000;

    /// @dev The same table complemented to 255, so that a leading-zero count
    /// is the byte position exclusive-ored with the slot.
    bytes32 private constant _LEADING =
        0xf8f9f9faf9fdfafbf9fdfcfdfafbfcfef9fafdfafcfcfbfefafafcfbffffffff;

    /// @dev The number of zero bits above the highest set bit of `x`; 256
    /// for zero.
    function leadingZeros(uint256 x) internal pure returns (uint256 r) {
        // Five comparisons find the byte holding the highest set bit without
        // a branch; the table finishes inside it. `r` only ever holds bits at
        // or above three and the slot value has all of those set, so the
        // exclusive-or subtracts. Zero reaches the last slot, 255, and the
        // final term makes it 256.
        r = x > type(uint128).max ? 128 : 0;
        r |= (x >> r) > type(uint64).max ? 64 : 0;
        r |= (x >> r) > type(uint32).max ? 32 : 0;
        r |= (x >> r) > type(uint16).max ? 16 : 0;
        r |= (x >> r) > type(uint8).max ? 8 : 0;
        unchecked {
            r = (r ^ uint8(_LEADING[(_SELECTOR >> (x >> r)) & 31])) + (x == 0 ? 1 : 0);
        }
    }

    /// @dev The index of the highest set bit of `x`, counting from zero at
    /// the low end; 256 for zero.
    function highestSetBit(uint256 x) internal pure returns (uint256 r) {
        // The same search as `leadingZeros`. Zero is seeded with 256: every
        // comparison is then false and its slot contributes nothing.
        r = (x == 0 ? 256 : 0) | (x > type(uint128).max ? 128 : 0);
        r |= (x >> r) > type(uint64).max ? 64 : 0;
        r |= (x >> r) > type(uint32).max ? 32 : 0;
        r |= (x >> r) > type(uint16).max ? 16 : 0;
        r |= (x >> r) > type(uint8).max ? 8 : 0;
        r |= uint8(_HIGHEST[(_SELECTOR >> (x >> r)) & 31]);
    }

    /// @dev The number of zero bits below the lowest set bit of `x`, which is
    /// that bit's index; 256 for zero.
    function trailingZeros(uint256 x) internal pure returns (uint256) {
        // Isolating the lowest set bit leaves a power of two, and the highest
        // set bit of that is the same bit. Zero stays zero.
        unchecked {
            return highestSetBit(x & (0 - x));
        }
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
