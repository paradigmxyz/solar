pragma solidity ^0.8.20;
// The contents of bytes literals and of packed encodings: their length and the bytes the
// source gives (a literal's, a fixed-width value's), at their offsets.
contract BytesContents {
    /// @custom:ensures out.length == 1 && uint8(out[0]) == 128
    function ok_hex_literal() external pure returns (bytes memory out) {
        return hex"80";
    }

    /// @custom:ensures out.length == 3 && uint8(out[0]) == 0x61 && uint8(out[2]) == 0x63
    function ok_string_literal() external pure returns (bytes memory out) {
        out = "abc";
    }

    /// @custom:ensures out.length == 1 && uint8(out[0]) == uint8(value)
    function ok_packed_uint8(uint256 value) external pure returns (bytes memory out) {
        return abi.encodePacked(uint8(value));
    }

    /// @custom:ensures out.length == 2 && uint8(out[0]) == value / 256 && uint8(out[1]) == value % 256
    function ok_packed_uint16(uint16 value) external pure returns (bytes memory out) {
        return abi.encodePacked(value);
    }

    /// @custom:ensures out.length == 22 && uint8(out[0]) == 0xd6 && uint8(out[1]) == 0x94 && uint8(out[21]) == uint160(a) % 256
    function ok_packed_literal_address(address a) external pure returns (bytes memory out) {
        return abi.encodePacked(hex"d694", a);
    }

    /// @custom:ensures out.length == a.length + 2 && uint8(out[a.length + 1]) == x % 256
    function ok_packed_after_bytes(bytes memory a, uint16 x) external pure returns (bytes memory out) {
        out = abi.encodePacked(a, x);
    }

    /// @custom:ensures uint8(out[0]) == 127
    function bad_hex_literal_byte() external pure returns (bytes memory out) {
        return hex"80";
    }

    /// @custom:ensures out.length == 2
    function bad_packed_uint8_length(uint256 value) external pure returns (bytes memory out) {
        return abi.encodePacked(uint8(value));
    }

    /// @custom:ensures uint8(out[0]) == value % 256
    function bad_packed_uint16_first(uint16 value) external pure returns (bytes memory out) {
        return abi.encodePacked(value);
    }

    /// @custom:ensures uint8(out[0]) == x % 256
    function bad_packed_after_bytes(bytes memory a, uint16 x) external pure returns (bytes memory out) {
        out = abi.encodePacked(a, x);
    }

    // The bytes another name writes are these bytes too: the write is refused rather than
    // leave this name's contents stale.
    /// @custom:ensures uint8(out[0]) == 128
    // expect: aliases `out`
    function bad_alias_write() external pure returns (bytes memory out) {
        out = hex"80";
        bytes memory other = out;
        other[0] = 0x01;
    }
}
