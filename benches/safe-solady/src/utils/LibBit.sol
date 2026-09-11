// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Checked Solidity implementation of the pinned Solady LibBit API.
/// @dev Raw boolean operations require clean boolean inputs, as upstream does.
library LibBit {
    function fls(uint256 x) internal pure returns (uint256 r) {
        if (x == 0) return 256;
        if (x >> 128 != 0) {
            x >>= 128;
            r = 128;
        }
        if (x >> 64 != 0) {
            x >>= 64;
            r |= 64;
        }
        if (x >> 32 != 0) {
            x >>= 32;
            r |= 32;
        }
        if (x >> 16 != 0) {
            x >>= 16;
            r |= 16;
        }
        if (x >> 8 != 0) {
            x >>= 8;
            r |= 8;
        }
        if (x >> 4 != 0) {
            x >>= 4;
            r |= 4;
        }
        if (x >> 2 != 0) {
            x >>= 2;
            r |= 2;
        }
        return r | (x >> 1);
    }

    function clz(uint256 x) internal pure returns (uint256 r) {
        return x == 0 ? 256 : 255 - fls(x);
    }

    function ffs(uint256 x) internal pure returns (uint256 r) {
        return x == 0 ? 256 : fls(x & (~x + 1));
    }

    function popCount(uint256 x) internal pure returns (uint256 c) {
        uint256 m1 = type(uint256).max / 3;
        uint256 m2 = type(uint256).max / 5;
        uint256 m4 = type(uint256).max / 17;
        x -= (x >> 1) & m1;
        x = (x & m2) + ((x >> 2) & m2);
        x = (x + (x >> 4)) & m4;
        return _sumBytes(x);
    }

    // Every input byte is at most eight. Pairing the halves bounds the
    // product below 2**256; the extracted byte is the sum modulo 256.
    function _sumBytes(uint256 x) private pure returns (uint256) {
        uint256 paired = (x & type(uint128).max) + (x >> 128);
        uint256 c = uint8((paired * (uint256(type(uint128).max) / 255)) >> 120);
        return c == 0 && x != 0 ? 256 : c;
    }

    function countZeroBytes(uint256 x) internal pure returns (uint256 c) {
        uint256 m = type(uint256).max / 255 * 127;
        return _sumBytes(~(((x & m) + m) | x | m) >> 7);
    }

    function countZeroBytes(bytes memory s) internal pure returns (uint256 c) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] == 0) ++c;
        }
    }

    function countZeroBytesCalldata(bytes calldata s) internal pure returns (uint256 c) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] == 0) ++c;
        }
    }

    function isPo2(uint256 x) internal pure returns (bool result) {
        return x != 0 && (x & (x - 1)) == 0;
    }

    function reverseBytes(uint256 x) internal pure returns (uint256 r) {
        uint256 m0 = 0x0000000000000000ffffffffffffffff0000000000000000ffffffffffffffff;
        uint256 m1 = m0 ^ (m0 << 32);
        uint256 m2 = m1 ^ (m1 << 16);
        uint256 m3 = m2 ^ (m2 << 8);
        r = (m3 & (x >> 8)) | ((m3 & x) << 8);
        r = (m2 & (r >> 16)) | ((m2 & r) << 16);
        r = (m1 & (r >> 32)) | ((m1 & r) << 32);
        r = (m0 & (r >> 64)) | ((m0 & r) << 64);
        return (r >> 128) | (r << 128);
    }

    function reverseBits(uint256 x) internal pure returns (uint256 r) {
        uint256 m0 = type(uint256).max / 17;
        uint256 m1 = m0 ^ (m0 << 2);
        uint256 m2 = m1 ^ (m1 << 1);
        r = reverseBytes(x);
        r = (m2 & (r >> 1)) | ((m2 & r) << 1);
        r = (m1 & (r >> 2)) | ((m1 & r) << 2);
        return (m0 & (r >> 4)) | ((m0 & r) << 4);
    }

    function commonBitPrefix(uint256 x, uint256 y) internal pure returns (uint256) {
        uint256 s = 256 - clz(x ^ y);
        return (x >> s) << s;
    }

    function commonNibblePrefix(uint256 x, uint256 y) internal pure returns (uint256) {
        uint256 s = (64 - (clz(x ^ y) >> 2)) << 2;
        return (x >> s) << s;
    }

    function commonBytePrefix(uint256 x, uint256 y) internal pure returns (uint256) {
        uint256 s = (32 - (clz(x ^ y) >> 3)) << 3;
        return (x >> s) << s;
    }

    function toNibbles(bytes memory s) internal pure returns (bytes memory result) {
        result = new bytes(s.length * 2);
        for (uint256 i; i < s.length; ++i) {
            result[i * 2] = s[i] >> 4;
            result[i * 2 + 1] = s[i] & 0x0f;
        }
    }

    function rawAnd(bool x, bool y) internal pure returns (bool z) {
        return x && y;
    }

    function and(bool x, bool y) internal pure returns (bool z) {
        return x && y;
    }

    function and(bool w, bool x, bool y) internal pure returns (bool z) {
        return w && x && y;
    }

    function and(bool v, bool w, bool x, bool y) internal pure returns (bool z) {
        return v && w && x && y;
    }

    function rawOr(bool x, bool y) internal pure returns (bool z) {
        return x || y;
    }

    function or(bool x, bool y) internal pure returns (bool z) {
        return x || y;
    }

    function or(bool w, bool x, bool y) internal pure returns (bool z) {
        return w || x || y;
    }

    function or(bool v, bool w, bool x, bool y) internal pure returns (bool z) {
        return v || w || x || y;
    }

    function rawToUint(bool b) internal pure returns (uint256 z) {
        return b ? 1 : 0;
    }

    function toUint(bool b) internal pure returns (uint256 z) {
        return b ? 1 : 0;
    }
}
