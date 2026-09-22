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

}
