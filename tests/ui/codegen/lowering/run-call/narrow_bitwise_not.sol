//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: notUint8(uint8) 0 => 255
//@[gas] run-call: notUint8(uint8) 0 => 255
//@[size] run-call: notUint8(uint8) 0 => 255
//@[none] run-call: notUint8(uint8) 0xa5 => 90
//@[gas] run-call: notUint8(uint8) 0xa5 => 90
//@[size] run-call: notUint8(uint8) 0xa5 => 90
//@[none] run-call: notUint16(uint16) 0 => 65535
//@[gas] run-call: notUint16(uint16) 0 => 65535
//@[size] run-call: notUint16(uint16) 0 => 65535
//@[none] run-call: notUint16(uint16) 0xa55a => 23205
//@[gas] run-call: notUint16(uint16) 0xa55a => 23205
//@[size] run-call: notUint16(uint16) 0xa55a => 23205
//@[none] run-call: notUint256(uint256) 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call: notUint256(uint256) 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[size] run-call: notUint256(uint256) 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[none] run-call: notInt8(int8) 0 => -1
//@[gas] run-call: notInt8(int8) 0 => -1
//@[size] run-call: notInt8(int8) 0 => -1
//@[none] run-call: notInt8(int8) -128 => 127
//@[gas] run-call: notInt8(int8) -128 => 127
//@[size] run-call: notInt8(int8) -128 => 127
//@[none] run-call: notInt16(int16) 4660 => -4661
//@[gas] run-call: notInt16(int16) 4660 => -4661
//@[size] run-call: notInt16(int16) 4660 => -4661
//@[none] run-call: notBytes1(bytes1) 0x00 => 0xff
//@[gas] run-call: notBytes1(bytes1) 0x00 => 0xff
//@[size] run-call: notBytes1(bytes1) 0x00 => 0xff
//@[none] run-call: notBytes1(bytes1) 0xa5 => 0x5a
//@[gas] run-call: notBytes1(bytes1) 0xa5 => 0x5a
//@[size] run-call: notBytes1(bytes1) 0xa5 => 0x5a
//@[none] run-call: notBytes2(bytes2) 0x0000 => 0xffff
//@[gas] run-call: notBytes2(bytes2) 0x0000 => 0xffff
//@[size] run-call: notBytes2(bytes2) 0x0000 => 0xffff
//@[none] run-call: notBytes2(bytes2) 0xa55a => 0x5aa5
//@[gas] run-call: notBytes2(bytes2) 0xa55a => 0x5aa5
//@[size] run-call: notBytes2(bytes2) 0xa55a => 0x5aa5
//@[none] run-call: compareBytes1(bytes1) 0xff => true
//@[gas] run-call: compareBytes1(bytes1) 0xff => true
//@[size] run-call: compareBytes1(bytes1) 0xff => true
//@[none] run-call: notBytes32(bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call: notBytes32(bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[size] run-call: notBytes32(bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[none] run-call: notLiteral() => -1
//@[gas] run-call: notLiteral() => -1
//@[size] run-call: notLiteral() => -1
//@[none] run-call: notLiteralEqualsNegativeOne() => true
//@[gas] run-call: notLiteralEqualsNegativeOne() => true
//@[size] run-call: notLiteralEqualsNegativeOne() => true

contract NarrowBitwiseNot {
    function notUint8(uint8 value) external pure returns (uint8) {
        return ~value;
    }

    function notUint16(uint16 value) external pure returns (uint16) {
        return ~value;
    }

    function notUint256(uint256 value) external pure returns (uint256) {
        return ~value;
    }

    function notInt8(int8 value) external pure returns (int8) {
        return ~value;
    }

    function notInt16(int16 value) external pure returns (int16) {
        return ~value;
    }

    function notBytes1(bytes1 value) external pure returns (bytes1) {
        return ~value;
    }

    function notBytes2(bytes2 value) external pure returns (bytes2) {
        return ~value;
    }

    function compareBytes1(bytes1 value) external pure returns (bool) {
        return (value >> 4) == bytes1(0x0f);
    }

    function notBytes32(bytes32 value) external pure returns (bytes32) {
        return ~value;
    }

    function notLiteral() external pure returns (int256) {
        return ~0;
    }

    function notLiteralEqualsNegativeOne() external pure returns (bool) {
        return ~0 == -1;
    }
}
