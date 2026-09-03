// A short hex literal converted to a wider fixed-bytes type must be
// right-padded: `bytes4(hex"01") == 0x01000000`.
// solc returns 0x01000000 for `constantHex()`; solar returns 0.
// Source: testdata/solidity/test/libsolidity/smtCheckerTests/typecast/string_literal_to_fixed_bytes_constant_initialization_1.sol
contract HexLiteralFixedBytesConstant {
    bytes4 public constant constantHex = hex"01";
    bytes4 public constant constantString = "a";

    function localHex() external pure returns (bytes4) {
        bytes4 value = hex"01";
        return value;
    }

    function localString() external pure returns (bytes4) {
        bytes4 value = "a";
        return value;
    }

    function castHex() external pure returns (bytes4) {
        return bytes4(hex"0102");
    }
}
