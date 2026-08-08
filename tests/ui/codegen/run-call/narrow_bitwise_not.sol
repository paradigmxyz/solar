//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: notUint8(uint8) 0 => 255
//@ run-call: notUint8(uint8) 0xa5 => 90
//@ run-call: notUint16(uint16) 0 => 65535
//@ run-call: notUint16(uint16) 0xa55a => 23205
//@ run-call: notUint256(uint256) 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: notInt8(int8) 0 => -1
//@ run-call: notInt8(int8) -128 => 127
//@ run-call: notInt16(int16) 4660 => -4661
//@ run-call: notBytes1(bytes1) 0x00 => 0xff
//@ run-call: notBytes1(bytes1) 0xa5 => 0x5a
//@ run-call: notBytes2(bytes2) 0x0000 => 0xffff
//@ run-call: notBytes2(bytes2) 0xa55a => 0x5aa5
//@ run-call: compareBytes1(bytes1) 0xff => true
//@ run-call: notBytes32(bytes32) 0x0000000000000000000000000000000000000000000000000000000000000000 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: notLiteral() => -1
//@ run-call: notLiteralEqualsNegativeOne() => true

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
