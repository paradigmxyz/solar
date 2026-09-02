//@ codegen-matrix: standard
//@ run-call: constantHex => 0x01000000
//@ run-call: constantString => 0x61000000
//@ run-call: localHex => 0x01000000
//@ run-call: localString => 0x61000000
//@ run-call: castHex => 0x01020000
// ported-from: test/libsolidity/smtCheckerTests/typecast/string_literal_to_fixed_bytes_constant_initialization_1.sol

contract FixedBytesConstants {
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
