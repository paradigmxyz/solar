// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Buffers, ByteBuilder} from "solar:core/v1/Buffers.sol";
import {Bytes} from "solar:core/v1/Bytes.sol";

/// @notice Text helpers over `string`.
/// @dev Compiler-owned module, imported as `solar:core/v1/Strings.sol`. This
/// is an ordinary source library over `Bytes` and `Buffers`; nothing here is
/// lowered specially. Strings are treated as bytes: no function validates or
/// depends on UTF-8, and `escapeJSON` escapes only what JSON requires, so a
/// valid UTF-8 input stays valid.
library Strings {
    using Buffers for ByteBuilder;

    bytes16 private constant DIGITS = "0123456789abcdef";

    /// @dev `value` in decimal.
    function toString(uint256 value) internal pure returns (string memory) {
        // Halving the remaining magnitude costs at most seven steps, where
        // dividing by ten once per digit costs up to seventy-eight.
        uint256 length = 1;
        uint256 x = value;
        if (x >= 1e64) {
            x /= 1e64;
            length += 64;
        }
        if (x >= 1e32) {
            x /= 1e32;
            length += 32;
        }
        if (x >= 1e16) {
            x /= 1e16;
            length += 16;
        }
        if (x >= 1e8) {
            x /= 1e8;
            length += 8;
        }
        if (x >= 1e4) {
            x /= 1e4;
            length += 4;
        }
        if (x >= 1e2) {
            x /= 1e2;
            length += 2;
        }
        if (x >= 10) ++length;
        bytes memory out = new bytes(length);
        do {
            --length;
            out[length] = bytes1(uint8(48 + value % 10));
            value /= 10;
        } while (length != 0);
        return string(out);
    }

    /// @dev `value` in decimal, with a leading `-` when negative.
    function toString(int256 value) internal pure returns (string memory) {
        if (value >= 0) return toString(uint256(value));
        // The magnitude of the most negative value does not fit `int256`.
        uint256 magnitude = value == type(int256).min ? uint256(1) << 255 : uint256(-value);
        return string.concat("-", toString(magnitude));
    }

    /// @dev Whether `a` and `b` are the same bytes.
    function equals(string memory a, string memory b) internal pure returns (bool) {
        return Bytes.equals(bytes(a), bytes(b));
    }

    /// @dev `s` with every character JSON requires escaped, without the
    /// surrounding quotes: the quote, the backslash, and the control
    /// characters, as the short forms where JSON has them and `\u00XX`
    /// elsewhere.
    function escapeJSON(string memory s) internal pure returns (string memory) {
        bytes memory input = bytes(s);
        ByteBuilder memory out = Buffers.create(input.length);
        for (uint256 i; i < input.length; ++i) {
            bytes1 c = input[i];
            if (c == '"' || c == "\\") {
                out.appendByte("\\");
                out.appendByte(c);
            } else if (uint8(c) >= 0x20) {
                out.appendByte(c);
            } else if (c == 0x08) {
                out.append("\\b");
            } else if (c == 0x09) {
                out.append("\\t");
            } else if (c == 0x0a) {
                out.append("\\n");
            } else if (c == 0x0c) {
                out.append("\\f");
            } else if (c == 0x0d) {
                out.append("\\r");
            } else {
                out.append("\\u00");
                out.appendByte(DIGITS[uint8(c) >> 4]);
                out.appendByte(DIGITS[uint8(c) & 15]);
            }
        }
        return string(out.finish());
    }
    /// @dev Returns `subject` with each non-overlapping occurrence of `needle`
    /// replaced by `replacement`.
    function replace(string memory subject, string memory needle, string memory replacement)
        internal
        pure
        returns (string memory)
    {
        bytes memory s = bytes(subject);
        bytes memory n = bytes(needle);
        bytes memory r = bytes(replacement);
        if (n.length > s.length) return subject;
        if (n.length == 0) {
            bytes memory expanded = new bytes(s.length + (s.length + 1) * r.length);
            uint256 o;
            for (uint256 i; i <= s.length; ++i) {
                Bytes.copyInto(expanded, o, r, 0, r.length);
                o += r.length;
                if (i < s.length) expanded[o++] = s[i];
            }
            return string(expanded);
        }
        uint256 count;
        for (uint256 i; i + n.length <= s.length;) {
            if (_matchAt(s, n, i)) {
                ++count;
                i += n.length;
            } else {
                ++i;
            }
        }
        bytes memory out = new bytes(s.length + count * r.length - count * n.length);
        uint256 at;
        uint256 copied;
        uint256 o;
        while (at + n.length <= s.length) {
            if (_matchAt(s, n, at)) {
                Bytes.copyInto(out, o, s, copied, at - copied);
                o += at - copied;
                Bytes.copyInto(out, o, r, 0, r.length);
                o += r.length;
                at += n.length;
                copied = at;
            } else {
                ++at;
            }
        }
        Bytes.copyInto(out, o, s, copied, s.length - copied);
        return string(out);
    }

    function _matchAt(bytes memory subject, bytes memory needle, uint256 at)
        private
        pure
        returns (bool)
    {
        for (uint256 i; i < needle.length; ++i) {
            if (subject[at + i] != needle[i]) return false;
        }
        return true;
    }

    /// @dev The byte offsets of each non-overlapping occurrence of `needle`.
    function indicesOf(string memory subject, string memory needle)
        internal
        pure
        returns (uint256[] memory)
    {
        bytes memory s = bytes(subject);
        bytes memory n = bytes(needle);
        if (n.length > s.length) return new uint256[](0);
        if (n.length == 0) {
            uint256[] memory every = new uint256[](s.length + 1);
            for (uint256 i; i <= s.length; ++i) every[i] = i;
            return every;
        }
        uint256 count;
        for (uint256 i; i + n.length <= s.length;) {
            if (_matchAt(s, n, i)) {
                ++count;
                i += n.length;
            } else {
                ++i;
            }
        }
        uint256[] memory found = new uint256[](count);
        count = 0;
        for (uint256 i; i + n.length <= s.length;) {
            if (_matchAt(s, n, i)) {
                found[count++] = i;
                i += n.length;
            } else {
                ++i;
            }
        }
        return found;
    }

    /// @dev The byte offset of the first occurrence of `needle` at or after
    /// `from`, or `type(uint256).max` when there is none. An empty needle is
    /// found at `from`, or at the end of a subject shorter than `from`.
    function indexOf(string memory subject, string memory needle, uint256 from)
        internal
        pure
        returns (uint256)
    {
        bytes memory s = bytes(subject);
        bytes memory n = bytes(needle);
        if (n.length == 0) return from > s.length ? s.length : from;
        if (n.length > s.length) return type(uint256).max;
        uint256 last = s.length - n.length;
        for (uint256 i = from; i <= last; ++i) {
            if (_matchAt(s, n, i)) return i;
        }
        return type(uint256).max;
    }

    /// @dev The byte offset of the last occurrence of `needle` at or before
    /// `from`, or `type(uint256).max` when there is none. An empty needle is
    /// found at `from`, or at the end of a subject shorter than `from`.
    function lastIndexOf(string memory subject, string memory needle, uint256 from)
        internal
        pure
        returns (uint256)
    {
        bytes memory s = bytes(subject);
        bytes memory n = bytes(needle);
        if (n.length > s.length) return type(uint256).max;
        uint256 last = s.length - n.length;
        if (from > last) from = last;
        for (uint256 i = from + 1; i != 0;) {
            --i;
            if (_matchAt(s, n, i)) return i;
        }
        return type(uint256).max;
    }

    /// @dev Splits `subject` at each non-overlapping `delimiter` occurrence.
    function split(string memory subject, string memory delimiter)
        internal
        pure
        returns (string[] memory result)
    {
        bytes memory s = bytes(subject);
        bytes memory d = bytes(delimiter);
        if (d.length == 0) {
            result = new string[](s.length);
            for (uint256 i; i < s.length; ++i) {
                bytes memory part = new bytes(1);
                part[0] = s[i];
                result[i] = string(part);
            }
            return result;
        }

        uint256[] memory indices = indicesOf(subject, delimiter);
        result = new string[](indices.length + 1);
        uint256 previous;
        for (uint256 i; i <= indices.length; ++i) {
            uint256 end = i == indices.length ? s.length : indices[i];
            bytes memory part = new bytes(end - previous);
            Bytes.copyInto(part, 0, s, previous, part.length);
            result[i] = string(part);
            previous = end + d.length;
        }
    }

    /// @dev The low `byteCount` bytes of a value do not hold all of it.
    error HexLengthInsufficient();

    /// @dev The low `byteCount` bytes of `value` as lowercase hexadecimal,
    /// two digits per byte, without a prefix. Reverts with
    /// `HexLengthInsufficient()` when `value` does not fit in them.
    function toHexStringNoPrefix(uint256 value, uint256 byteCount)
        internal
        pure
        returns (string memory)
    {
        bytes memory out = new bytes(byteCount * 2);
        uint256 i = byteCount;
        while (i != 0) {
            --i;
            out[i * 2 + 1] = DIGITS[value & 15];
            value >>= 4;
            out[i * 2] = DIGITS[value & 15];
            value >>= 4;
        }
        if (value != 0) revert HexLengthInsufficient();
        return string(out);
    }

    /// @dev `toHexStringNoPrefix(value, byteCount)` after `0x`.
    function toHexString(uint256 value, uint256 byteCount) internal pure returns (string memory) {
        return string.concat("0x", toHexStringNoPrefix(value, byteCount));
    }

    /// @dev `value` as lowercase hexadecimal in its fewest whole bytes, at
    /// least one, without a prefix.
    function toHexStringNoPrefix(uint256 value) internal pure returns (string memory) {
        uint256 length = 1;
        for (uint256 x = value; x > 255; x >>= 8) ++length;
        return toHexStringNoPrefix(value, length);
    }

    /// @dev `toHexStringNoPrefix(value)` after `0x`.
    function toHexString(uint256 value) internal pure returns (string memory) {
        return string.concat("0x", toHexStringNoPrefix(value));
    }

    /// @dev Minimal lowercase hexadecimal digits without a prefix.
    function toMinimalHexStringNoPrefix(uint256 value) internal pure returns (string memory) {
        uint256 length = 1;
        for (uint256 x = value; x > 15; x >>= 4) ++length;
        bytes memory out = new bytes(length);
        while (length != 0) {
            --length;
            out[length] = DIGITS[value & 15];
            value >>= 4;
        }
        return string(out);
    }

    /// @dev Minimal lowercase hexadecimal digits after `0x`.
    function toMinimalHexString(uint256 value) internal pure returns (string memory) {
        return string.concat("0x", toMinimalHexStringNoPrefix(value));
    }

    /// @dev Packs a nonempty string of at most 31 bytes into one word. The
    /// first byte is its length and the remaining bytes are its contents.
    function packOne(string memory value) internal pure returns (bytes32 result) {
        bytes memory input = bytes(value);
        if (input.length == 0 || input.length > 31) return 0;
        result = bytes32(input.length << 248);
        for (uint256 i; i < input.length; ++i) {
            result |= bytes32(uint256(uint8(input[i])) << ((30 - i) * 8));
        }
    }

    /// @dev Reconstructs a string produced by {packOne}.
    function unpackOne(bytes32 packed) internal pure returns (string memory result) {
        uint256 length = uint8(packed[0]);
        if (length > 31) length = 31;
        bytes memory out = new bytes(length);
        for (uint256 i; i < length; ++i) out[i] = packed[i + 1];
        return string(out);
    }

    /// @dev Packs two strings whose combined length is 1..30 bytes into one
    /// word. Each string is preceded by one length byte.
    function packTwo(string memory a, string memory b) internal pure returns (bytes32 result) {
        bytes memory x = bytes(a);
        bytes memory y = bytes(b);
        uint256 total = x.length + y.length;
        if (total == 0 || total > 30) return 0;
        result = bytes32(x.length << 248);
        for (uint256 i; i < x.length; ++i) {
            result |= bytes32(uint256(uint8(x[i])) << ((30 - i) * 8));
        }
        result |= bytes32(y.length << ((30 - x.length) * 8));
        for (uint256 i; i < y.length; ++i) {
            result |= bytes32(uint256(uint8(y[i])) << ((29 - x.length - i) * 8));
        }
    }

    /// @dev Reconstructs two strings produced by {packTwo}.
    function unpackTwo(bytes32 packed)
        internal
        pure
        returns (string memory resultA, string memory resultB)
    {
        uint256 aLength = uint8(packed[0]);
        if (aLength > 30) aLength = 30;
        bytes memory a = new bytes(aLength);
        for (uint256 i; i < aLength; ++i) a[i] = packed[i + 1];
        uint256 bLength = uint8(packed[aLength + 1]);
        if (bLength > 30 - aLength) bLength = 30 - aLength;
        bytes memory b = new bytes(bLength);
        for (uint256 i; i < bLength; ++i) b[i] = packed[aLength + i + 2];
        return (string(a), string(b));
    }

}
