// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice Hexadecimal text over `bytes`.
/// @dev Compiler-owned module, imported as `solar:core/v1/codecs/Hex.sol`.
/// `encode` and `encodePrefixed` may use specialized lowering; these checked
/// bodies define the portable behavior and run under `-Zno-core-intrinsics`.
/// `decode` is an ordinary source function over `Bytes`. `encode` writes
/// lowercase digits, two to a byte. `decode` is
/// strict: it accepts either case and an optional `0x` prefix, and reverts
/// with `InvalidHex()` on any other character or an odd number of digits.
library Hex {
    /// @dev The input is not hexadecimal.
    error InvalidHex();

    bytes16 private constant DIGITS = "0123456789abcdef";

    /// @dev Nibble-spreading masks, one per halving of the packing.
    uint256 private constant MASK_64 =
        0x0000000000000000ffffffffffffffff0000000000000000ffffffffffffffff;
    uint256 private constant MASK_32 =
        0x00000000ffffffff00000000ffffffff00000000ffffffff00000000ffffffff;
    uint256 private constant MASK_16 =
        0x0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff;
    uint256 private constant MASK_8 =
        0x00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff;
    uint256 private constant MASK_4 =
        0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f;

    /// @dev One copy of the value in every byte.
    uint256 private constant SPREAD_1 =
        0x0101010101010101010101010101010101010101010101010101010101010101;
    uint256 private constant SPREAD_6 =
        0x0606060606060606060606060606060606060606060606060606060606060606;
    uint256 private constant SPREAD_ASCII_0 =
        0x3030303030303030303030303030303030303030303030303030303030303030;

    /// @dev Digit value per byte, and 0xff where the byte is not a digit. Every
    /// invalid entry has its top bit set, so one test after the loop finds any.
    bytes private constant DECODE = hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffff00010203040506070809ffffffffffff"
        hex"ff0a0b0c0d0e0fffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ff0a0b0c0d0e0fffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    /// @dev The digits of `data`, without a prefix.
    function encode(bytes memory data) internal pure returns (string memory) {
        bytes memory out = new bytes(data.length * 2);
        _encodeInto(out, 0, data);
        return string(out);
    }

    /// @dev The digits of `data` after `0x`.
    function encodePrefixed(bytes memory data) internal pure returns (string memory) {
        bytes memory out = new bytes(data.length * 2 + 2);
        Bytes.writeBytes2(out, 0, "0x");
        _encodeInto(out, 2, data);
        return string(out);
    }

    /// @dev The bytes `data` spells. Reverts when `data` is not hexadecimal.
    function decode(string memory data) internal pure returns (bytes memory result) {
        bytes memory input = bytes(data);
        uint256 n = input.length;
        uint256 i;
        if (n >= 2 && input[0] == "0" && (input[1] == "x" || input[1] == "X")) i = 2;
        if ((n - i) % 2 != 0) revert InvalidHex();
        result = new bytes((n - i) / 2);
        bytes memory table = DECODE;
        // Every digit is below sixteen and every invalid entry is 0xff, so
        // the accumulated top bit says whether any character was invalid.
        uint256 seen;
        for (uint256 j; i < n; ++j) {
            uint256 high = uint8(table[uint8(input[i])]);
            uint256 low = uint8(table[uint8(input[i + 1])]);
            seen |= high | low;
            result[j] = bytes1(uint8((high << 4) | (low & 15)));
            i += 2;
        }
        if (seen & 0x80 != 0) revert InvalidHex();
    }

    /// @dev Writes the digits of `data` into `out` from `start`. Sixteen bytes
    /// make one word of digits; a remainder is one more block pulled back to
    /// end where the input ends, and an input below sixteen bytes goes a byte
    /// at a time.
    function _encodeInto(bytes memory out, uint256 start, bytes memory data) private pure {
        uint256 n = data.length;
        uint256 i;
        uint256 o = start;
        while (i + 16 <= n) {
            Bytes.writeBytes32(out, o, _digits(uint128(Bytes.readBytes16(data, i))));
            i += 16;
            o += 32;
        }
        if (i == n) return;
        if (n >= 16) {
            uint256 last = n - 16;
            Bytes.writeBytes32(out, start + last * 2, _digits(uint128(Bytes.readBytes16(data, last))));
            return;
        }
        for (; i < n; ++i) {
            uint256 x = uint8(data[i]);
            out[o] = DIGITS[x >> 4];
            out[o + 1] = DIGITS[x & 15];
            o += 2;
        }
    }

    /// @dev The thirty-two digits of the sixteen bytes in `x`, which must be
    /// below `2 ** 128`.
    function _digits(uint256 x) private pure returns (bytes32) {
        // Spread the thirty-two nibbles one to a byte, halving the packing
        // each step: eight bytes, four, two, one, then one nibble.
        x = (x | (x << 64)) & MASK_64;
        x = (x | (x << 32)) & MASK_32;
        x = (x | (x << 16)) & MASK_16;
        x = (x | (x << 8)) & MASK_8;
        x = (x | (x << 4)) & MASK_4;
        // Adding six carries every nibble above nine into the next bit, which
        // marks the ones that become letters rather than digits.
        uint256 letters = ((x + SPREAD_6) >> 4) & SPREAD_1;
        return bytes32(x + SPREAD_ASCII_0 + letters * 39);
    }
}
