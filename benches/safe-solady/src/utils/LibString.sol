// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Checked Solidity replacements for the value-oriented LibString APIs.
/// @dev Storage reinterpretation and direct-return APIs are deliberately absent.
library LibString {
    error HexLengthInsufficient();
    error TooBigForSmallString();
    error StringNot7BitASCII();
    uint256 internal constant NOT_FOUND = type(uint256).max;
    bytes16 private constant HEX = "0123456789abcdef";

    function toString(uint256 value) internal pure returns (string memory result) {
        uint256 length = 1;
        for (uint256 x = value; x >= 10; x /= 10) {
            ++length;
        }
        bytes memory out = new bytes(length);
        do {
            --length;
            out[length] = bytes1(uint8(48 + value % 10));
            value /= 10;
        } while (length != 0);
        return string(out);
    }

    function toString(int256 value) internal pure returns (string memory result) {
        if (value >= 0) return toString(uint256(value));
        return string.concat("-", toString(uint256(-(value + 1)) + 1));
    }

    function toHexStringNoPrefix(uint256 value, uint256 byteCount) internal pure returns (string memory result) {
        bytes memory out = new bytes(byteCount * 2);
        for (uint256 i = out.length; i != 0;) {
            --i;
            out[i] = HEX[value & 15];
            value >>= 4;
        }
        if (value != 0) revert HexLengthInsufficient();
        return string(out);
    }

    function toHexString(uint256 value, uint256 byteCount) internal pure returns (string memory result) {
        return string.concat("0x", toHexStringNoPrefix(value, byteCount));
    }

    function toHexStringNoPrefix(uint256 value) internal pure returns (string memory result) {
        uint256 length = 1;
        for (uint256 x = value; x > 255; x >>= 8) {
            ++length;
        }
        return toHexStringNoPrefix(value, length);
    }

    function toHexString(uint256 value) internal pure returns (string memory result) {
        return string.concat("0x", toHexStringNoPrefix(value));
    }

    function toMinimalHexStringNoPrefix(uint256 value) internal pure returns (string memory result) {
        uint256 length = 1;
        for (uint256 x = value; x > 15; x >>= 4) {
            ++length;
        }
        bytes memory out = new bytes(length);
        while (length != 0) {
            --length;
            out[length] = HEX[value & 15];
            value >>= 4;
        }
        return string(out);
    }

    function toMinimalHexString(uint256 value) internal pure returns (string memory result) {
        return string.concat("0x", toMinimalHexStringNoPrefix(value));
    }

    function toHexStringNoPrefix(address value) internal pure returns (string memory result) {
        return toHexStringNoPrefix(uint160(value), 20);
    }

    function toHexString(address value) internal pure returns (string memory result) {
        return toHexString(uint160(value), 20);
    }

    function toHexStringNoPrefix(bytes memory raw) internal pure returns (string memory result) {
        bytes memory out = new bytes(raw.length * 2);
        for (uint256 i; i < raw.length; ++i) {
            uint256 x = uint8(raw[i]);
            out[2 * i] = HEX[x >> 4];
            out[2 * i + 1] = HEX[x & 15];
        }
        return string(out);
    }

    function toHexString(bytes memory raw) internal pure returns (string memory result) {
        return string.concat("0x", toHexStringNoPrefix(raw));
    }

    function is7BitASCII(string memory s) internal pure returns (bool result) {
        bytes memory b = bytes(s);
        for (uint256 i; i < b.length; ++i) {
            if (b[i] > 0x7f) return false;
        }
        return true;
    }

    function is7BitASCII(string memory s, uint128 allowed) internal pure returns (bool result) {
        bytes memory b = bytes(s);
        for (uint256 i; i < b.length; ++i) {
            if (((allowed >> uint8(b[i])) & 1) == 0) return false;
        }
        return true;
    }

    function to7BitASCIIAllowedLookup(string memory s) internal pure returns (uint128 result) {
        bytes memory b = bytes(s);
        for (uint256 i; i < b.length; ++i) {
            if (b[i] > 0x7f) revert StringNot7BitASCII();
            result |= uint128(1) << uint8(b[i]);
        }
    }

    function runeCount(string memory s) internal pure returns (uint256 result) {
        bytes memory b = bytes(s);
        for (uint256 i; i < b.length; ++result) {
            uint256 c = uint8(b[i]);
            i += c < 0xc0 ? 1 : c < 0xe0 ? 2 : c < 0xf0 ? 3 : c < 0xf8 ? 4 : c < 0xfc ? 5 : 6;
        }
    }

    function concat(string memory a, string memory b) internal pure returns (string memory) {
        return string.concat(a, b);
    }

    function eq(string memory a, string memory b) internal pure returns (bool result) {
        return keccak256(bytes(a)) == keccak256(bytes(b));
    }

    function toCase(string memory subject, bool toUpper) internal pure returns (string memory result) {
        bytes memory b = bytes(subject);
        bytes memory out = new bytes(b.length);
        for (uint256 i; i < b.length; ++i) {
            uint8 c = uint8(b[i]);
            if (toUpper && c >= 97 && c <= 122) c -= 32;
            if (!toUpper && c >= 65 && c <= 90) c += 32;
            out[i] = bytes1(c);
        }
        return string(out);
    }

    function lower(string memory subject) internal pure returns (string memory result) {
        return toCase(subject, false);
    }

    function upper(string memory subject) internal pure returns (string memory result) {
        return toCase(subject, true);
    }
}
