// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Checked Solidity implementation of the pinned Solady Base64 API.
/// @dev Decode accepts the documented standard, URL and IMAP alphabets and
/// padding modes. Invalid input has unspecified output in the upstream API.
library Base64 {
    bytes32 private constant ENCODE0 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";
    bytes32 private constant ENCODE1 = "ghijklmnopqrstuvwxyz0123456789+/";

    function encode(bytes memory data, bool fileSafe, bool noPadding) internal pure returns (string memory result) {
        uint256 n = data.length;
        uint256 padding = n % 3 == 0 ? 0 : 3 - n % 3;
        uint256 length = ((n + 2) / 3) * 4;
        if (noPadding) length -= padding;
        bytes memory out = new bytes(length);
        uint256 j;
        for (uint256 i; i < n; i += 3) {
            uint256 word = uint256(uint8(data[i])) << 16;
            if (i + 1 < n) word |= uint256(uint8(data[i + 1])) << 8;
            if (i + 2 < n) word |= uint8(data[i + 2]);
            out[j] = _encode((word >> 18) & 63, fileSafe);
            out[j + 1] = _encode((word >> 12) & 63, fileSafe);
            if (j + 2 < length) out[j + 2] = i + 1 < n ? _encode((word >> 6) & 63, fileSafe) : bytes1("=");
            if (j + 3 < length) out[j + 3] = i + 2 < n ? _encode(word & 63, fileSafe) : bytes1("=");
            j += 4;
        }
        return string(out);
    }

    function _encode(uint256 index, bool fileSafe) private pure returns (bytes1) {
        if (fileSafe && index >= 62) return index == 62 ? bytes1("-") : bytes1("_");
        return index < 32 ? ENCODE0[index] : ENCODE1[index & 31];
    }

    function encode(bytes memory data) internal pure returns (string memory result) {
        return encode(data, false, false);
    }

    function encode(bytes memory data, bool fileSafe) internal pure returns (string memory result) {
        return encode(data, fileSafe, false);
    }

    function decode(string memory data) internal pure returns (bytes memory result) {
        bytes memory input = bytes(data);
        uint256 n = input.length;
        if (n == 0) return new bytes(0);
        uint256 length = (n / 4) * 3;
        uint256 tail = n % 4;
        if (tail != 0) {
            length += tail - 1;
        } else {
            if (input[n - 1] == "=") --length;
            if (input[n - 2] == "=") --length;
        }
        result = new bytes(length);
        uint256 j;
        for (uint256 i; i < n; i += 4) {
            uint256 word = _decode(input[i]) << 18;
            if (i + 1 < n) word |= _decode(input[i + 1]) << 12;
            if (i + 2 < n) word |= _decode(input[i + 2]) << 6;
            if (i + 3 < n) word |= _decode(input[i + 3]);
            if (j < length) result[j] = bytes1(uint8(word >> 16));
            if (j + 1 < length) result[j + 1] = bytes1(uint8(word >> 8));
            if (j + 2 < length) result[j + 2] = bytes1(uint8(word));
            j += 3;
        }
    }

    bytes32 private constant DECODE1 = hex"00000000000000000000003e3f3e003f3435363738393a3b3c3d000000000000";
    bytes32 private constant DECODE2 = hex"00000102030405060708090a0b0c0d0e0f10111213141516171819000000003f";
    bytes32 private constant DECODE3 = hex"001a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132330000000000";

    function _decode(bytes1 c) private pure returns (uint256) {
        uint256 x = uint8(c);
        if (x < 32 || x >= 128) return 0;
        bytes32 table = x < 64 ? DECODE1 : x < 96 ? DECODE2 : DECODE3;
        return uint8(table[x & 31]);
    }
}
