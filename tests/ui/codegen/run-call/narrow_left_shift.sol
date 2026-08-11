//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: leftU8(uint8,uint8) 255, 0 => 255
//@ run-call: leftU8(uint8,uint8) 255, 1 => 254
//@ run-call: leftU8(uint8,uint8) 255, 8 => 0
//@ run-call: leftU8(uint8,uint8) 1, 255 => 0
//@ run-call: leftU16(uint16,uint8) 32769, 1 => 2
//@ run-call: leftU16(uint16,uint8) 1, 16 => 0
//@ run-call: leftI8(int8,uint8) 1, 7 => -128
//@ run-call: leftI8(int8,uint8) -1, 1 => -2
//@ run-call: leftI8(int8,uint8) -1, 8 => 0
//@ run-call: leftI16(int16,uint8) 16384, 1 => -32768
//@ run-call: leftI16(int16,uint8) -1, 1 => -2
//@ run-call: leftI16(int16,uint8) -1, 16 => 0
//@ run-call: uncheckedU8(uint8,uint8) 255, 1 => 254
//@ run-call: uncheckedU8(uint8,uint8) 255, 8 => 0
//@ run-call: leftU256(uint256,uint256) 1, 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: leftU256(uint256,uint256) 1, 256 => 0
//@ run-call: rightU8(uint8,uint8) 128, 7 => 1
//@ run-call: rightI8(int8,uint8) -128, 7 => -1

contract NarrowLeftShift {
    function leftU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value << bits;
    }

    function leftU16(uint16 value, uint8 bits) external pure returns (uint16) {
        return value << bits;
    }

    function leftI8(int8 value, uint8 bits) external pure returns (int8) {
        return value << bits;
    }

    function leftI16(int16 value, uint8 bits) external pure returns (int16) {
        return value << bits;
    }

    function uncheckedU8(uint8 value, uint8 bits) external pure returns (uint8 result) {
        unchecked {
            result = value << bits;
        }
    }

    function leftU256(uint256 value, uint256 bits) external pure returns (uint256) {
        return value << bits;
    }

    function rightU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value >> bits;
    }

    function rightI8(int8 value, uint8 bits) external pure returns (int8) {
        return value >> bits;
    }
}
