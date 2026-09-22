// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice Base64 over `bytes`.
/// @dev Compiler-owned module, imported as `solar:core/v1/codecs/Base64.sol`.
/// Calls may use specialized codec lowering; these checked bodies define
/// the portable behavior and run under `-Zno-core-intrinsics`. `encode` writes
/// the standard alphabet with padding, or the
/// URL-safe one and no padding on request. `decode` is strict: it accepts
/// either alphabet, with or without padding, and reverts with
/// `InvalidBase64()` on a character outside both, padding anywhere but the
/// end of a padded input, or a length no encoding has. It does not reject
/// unused low bits in a final group, which RFC 4648 leaves to the decoder.
library Base64 {
    /// @dev The input is not Base64.
    error InvalidBase64();

    /// @dev The standard alphabet. The two URL-safe replacements are patched
    /// into a single memory copy so every lookup in a call shares one buffer.
    string private constant ENCODE =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// @dev Sextet per byte value, and 0xff where the byte is in neither
    /// alphabet: codes 43 and 45 are 62, codes 47 and 95 are 63. The table spans
    /// the byte range so a lookup needs no range test, and every invalid entry
    /// has its top bit set so that one test after the loop finds any of them.
    bytes private constant DECODE = hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffff3eff3eff3f3435363738393a3b3c3dffffffffffff"
        hex"ff000102030405060708090a0b0c0d0e0f10111213141516171819ffffffff3f"
        hex"ff1a1b1c1d1e1f202122232425262728292a2b2c2d2e2f30313233ffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        hex"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    /// @dev One copy of the value in every byte.
    uint256 private constant ONES =
        0x0101010101010101010101010101010101010101010101010101010101010101;

    /// @dev Masks for spreading twenty-four packed bytes into eight groups of
    /// three, one group to each four-byte lane. Each step moves the upper half
    /// of every group up and leaves the lower half where it is.
    uint256 private constant MASK_96 =
        0x0000000000000000000000000000000000000000ffffffffffffffffffffffff;
    uint256 private constant SPREAD_HIGH_48 =
        0x00000000ffffffffffff00000000000000000000ffffffffffff000000000000;
    uint256 private constant SPREAD_LOW_48 =
        0x00000000000000000000ffffffffffff00000000000000000000ffffffffffff;
    uint256 private constant SPREAD_HIGH_24 =
        0x0000ffffff0000000000ffffff0000000000ffffff0000000000ffffff000000;
    uint256 private constant SPREAD_LOW_24 =
        0x0000000000ffffff0000000000ffffff0000000000ffffff0000000000ffffff;

    /// @dev Six set bits at the bottom of every four-byte lane.
    uint256 private constant LANE_63 =
        0x0000003f0000003f0000003f0000003f0000003f0000003f0000003f0000003f;

    /// @dev Input length from which converting a word at a time pays for
    /// itself. Below it the per-block work outweighs the characters it saves.
    uint256 private constant WIDE_THRESHOLD = 96;

    function encode(bytes memory data, bool fileSafe, bool noPadding)
        internal
        pure
        returns (string memory result)
    {
        if (data.length >= WIDE_THRESHOLD) return _encodeWide(data, fileSafe, noPadding);
        uint256 n = data.length;
        uint256 padding = n % 3 == 0 ? 0 : 3 - n % 3;
        uint256 length = ((n + 2) / 3) * 4;
        if (noPadding) length -= padding;
        bytes memory out = new bytes(length);
        if (n == 0) return string(out);
        bytes memory table = bytes(ENCODE);
        if (fileSafe) {
            // Written as byte values: a one-character string literal is first
            // materialized as a `bytes` value before the `bytes1` conversion.
            table[62] = bytes1(uint8(0x2d));
            table[63] = bytes1(uint8(0x5f));
        }
        uint256 i;
        uint256 j;
        while (i + 2 < n && j + 3 < length) {
            uint256 word = (uint256(uint8(data[i])) << 16) | (uint256(uint8(data[i + 1])) << 8)
                | uint256(uint8(data[i + 2]));
            out[j] = table[word >> 18];
            out[j + 1] = table[(word >> 12) & 63];
            out[j + 2] = table[(word >> 6) & 63];
            out[j + 3] = table[word & 63];
            i += 3;
            j += 4;
        }
        if (i < n) {
            uint256 word = uint256(uint8(data[i])) << 16;
            if (i + 1 < n) word |= uint256(uint8(data[i + 1])) << 8;
            if (i + 2 < n) word |= uint8(data[i + 2]);
            out[j] = table[word >> 18];
            out[j + 1] = table[(word >> 12) & 63];
            if (j + 2 < length) {
                out[j + 2] = i + 1 < n ? table[(word >> 6) & 63] : bytes1("=");
            }
            if (j + 3 < length) {
                out[j + 3] = i + 2 < n ? table[word & 63] : bytes1("=");
            }
        }
        return string(out);
    }

    /// @dev `encode` for inputs long enough that converting twenty-four bytes
    /// at a time pays. Kept apart from the short path so that path's code is
    /// not disturbed by the wider one's register pressure.
    function _encodeWide(bytes memory data, bool fileSafe, bool noPadding)
        private
        pure
        returns (string memory result)
    {
        uint256 n = data.length;
        uint256 padding = n % 3 == 0 ? 0 : 3 - n % 3;
        uint256 length = ((n + 2) / 3) * 4;
        if (noPadding) length -= padding;
        bytes memory out = new bytes(length);
        uint256 i;
        uint256 j;
        // Twenty-four bytes are thirty-two characters, one word of output, so
        // they are converted and stored together.
        while (i + 24 <= n && j + 32 <= length) {
            bytes32 chars = _encodeWord(uint256(uint192(Bytes.readBytes24(data, i))), fileSafe);
            Bytes.writeBytes32(out, j, chars);
            i += 24;
            j += 32;
        }
        bytes memory table = bytes(ENCODE);
        if (fileSafe) {
            table[62] = bytes1(uint8(0x2d));
            table[63] = bytes1(uint8(0x5f));
        }
        while (i + 2 < n && j + 3 < length) {
            uint256 word = (uint256(uint8(data[i])) << 16) | (uint256(uint8(data[i + 1])) << 8)
                | uint256(uint8(data[i + 2]));
            out[j] = table[word >> 18];
            out[j + 1] = table[(word >> 12) & 63];
            out[j + 2] = table[(word >> 6) & 63];
            out[j + 3] = table[word & 63];
            i += 3;
            j += 4;
        }
        if (i < n) {
            uint256 word = uint256(uint8(data[i])) << 16;
            if (i + 1 < n) word |= uint256(uint8(data[i + 1])) << 8;
            if (i + 2 < n) word |= uint8(data[i + 2]);
            out[j] = table[word >> 18];
            out[j + 1] = table[(word >> 12) & 63];
            if (j + 2 < length) {
                out[j + 2] = i + 1 < n ? table[(word >> 6) & 63] : bytes1("=");
            }
            if (j + 3 < length) {
                out[j + 3] = i + 2 < n ? table[word & 63] : bytes1("=");
            }
        }
        return string(out);
    }

    /// @dev The thirty-two characters of the twenty-four bytes packed into the
    /// low bits of `packed`.
    function _encodeWord(uint256 packed, bool fileSafe) private pure returns (bytes32) {
        // The URL-safe alphabet differs only in its last two characters.
        uint256 stepDown = fileSafe ? 13 : 15;
        uint256 stepUp = fileSafe ? 49 : 3;
        // Spread the eight groups of three bytes one to a four-byte lane,
        // halving the packing each step.
        uint256 x = ((packed >> 96) << 128) | (packed & MASK_96);
        x = ((x & SPREAD_HIGH_48) << 16) | (x & SPREAD_LOW_48);
        x = ((x & SPREAD_HIGH_24) << 8) | (x & SPREAD_LOW_24);
        // Cut each lane's twenty-four bits into four six-bit values, one to a
        // byte. Shifting the whole word and masking per lane keeps the
        // neighbours out.
        uint256 v = (((x >> 18) & LANE_63) << 24) | (((x >> 12) & LANE_63) << 16)
            | (((x >> 6) & LANE_63) << 8) | (x & LANE_63);
        // Each threshold the alphabet steps at is found by adding its distance
        // to 128, which carries into a byte's high bit exactly at the
        // threshold: a one marks every byte at or above it. No adjustment
        // carries out of its own byte, so the whole word steps at once.
        uint256 letter = ((v + 102 * ONES) >> 7) & ONES;
        uint256 digit = ((v + 76 * ONES) >> 7) & ONES;
        uint256 last2 = ((v + 66 * ONES) >> 7) & ONES;
        uint256 last1 = ((v + 65 * ONES) >> 7) & ONES;
        return bytes32(v + 65 * ONES + 6 * letter - 75 * digit - stepDown * last2 + stepUp * last1);
    }

    function encode(bytes memory data) internal pure returns (string memory result) {
        return encode(data, false, false);
    }

    function encode(bytes memory data, bool fileSafe) internal pure returns (string memory result) {
        return encode(data, fileSafe, false);
    }

    /// @dev The bytes `data` encodes. Reverts when `data` is not Base64.
    function decode(string memory data) internal pure returns (bytes memory result) {
        return decode(data, false);
    }

    /// @dev With `imap` enabled, also accept comma as the last alphabet symbol.
    /// All other validation is identical to the one-argument form.
    function decode(string memory data, bool imap) internal pure returns (bytes memory result) {
        bytes memory input = bytes(data);
        uint256 n = input.length;
        // Padding is at most two characters, at the end of a padded input.
        if (n != 0 && n % 4 == 0 && input[n - 1] == "=") {
            --n;
            if (input[n - 1] == "=") --n;
        }
        uint256 tail = n % 4;
        if (tail == 1) revert InvalidBase64();
        uint256 length = (n / 4) * 3;
        if (tail != 0) length += tail - 1;
        result = new bytes(length);
        bytes memory table = DECODE;
        if (imap) table[44] = bytes1(uint8(63));
        // Every sextet is below 64 and every invalid entry is 0xff, so the
        // accumulated top bit says whether any character was invalid.
        uint256 seen;
        uint256 i;
        uint256 j;
        while (i + 4 <= n) {
            uint256 a = uint8(table[uint8(input[i])]);
            uint256 b = uint8(table[uint8(input[i + 1])]);
            uint256 c = uint8(table[uint8(input[i + 2])]);
            uint256 d = uint8(table[uint8(input[i + 3])]);
            seen |= a | b | c | d;
            uint256 word = (a << 18) | (b << 12) | (c << 6) | d;
            result[j] = bytes1(uint8(word >> 16));
            result[j + 1] = bytes1(uint8(word >> 8));
            result[j + 2] = bytes1(uint8(word));
            i += 4;
            j += 3;
        }
        if (tail != 0) {
            uint256 a = uint8(table[uint8(input[i])]);
            uint256 b = uint8(table[uint8(input[i + 1])]);
            seen |= a | b;
            uint256 word = (a << 18) | (b << 12);
            if (tail == 3) {
                uint256 c = uint8(table[uint8(input[i + 2])]);
                seen |= c;
                word |= c << 6;
                result[j + 1] = bytes1(uint8(word >> 8));
            }
            result[j] = bytes1(uint8(word >> 16));
        }
        if (seen & 0x80 != 0) revert InvalidBase64();
    }
}
