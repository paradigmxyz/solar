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
//@[none] run-call: int8ToUint256(int8) -1 => 255
//@[gas] run-call: int8ToUint256(int8) -1 => 255
//@[size] run-call: int8ToUint256(int8) -1 => 255
//@[none] run-call: int8ToUint256(int8) -128 => 128
//@[gas] run-call: int8ToUint256(int8) -128 => 128
//@[size] run-call: int8ToUint256(int8) -128 => 128
//@[none] run-call: int8ToUint256(int8) 127 => 127
//@[gas] run-call: int8ToUint256(int8) 127 => 127
//@[size] run-call: int8ToUint256(int8) 127 => 127
//@[none] run-call: int24ToUint256(int24) -1 => 16777215
//@[gas] run-call: int24ToUint256(int24) -1 => 16777215
//@[size] run-call: int24ToUint256(int24) -1 => 16777215
//@[none] run-call: int24ToUint256(int24) -8388608 => 8388608
//@[gas] run-call: int24ToUint256(int24) -8388608 => 8388608
//@[size] run-call: int24ToUint256(int24) -8388608 => 8388608
//@[none] run-call: int24ToUint256(int24) 8388607 => 8388607
//@[gas] run-call: int24ToUint256(int24) 8388607 => 8388607
//@[size] run-call: int24ToUint256(int24) 8388607 => 8388607
//@[none] run-call: uint8ToInt256(uint8) 255 => -1
//@[gas] run-call: uint8ToInt256(uint8) 255 => -1
//@[size] run-call: uint8ToInt256(uint8) 255 => -1
//@[none] run-call: uint8ToInt256(uint8) 128 => -128
//@[gas] run-call: uint8ToInt256(uint8) 128 => -128
//@[size] run-call: uint8ToInt256(uint8) 128 => -128
//@[none] run-call: uint8ToInt256(uint8) 127 => 127
//@[gas] run-call: uint8ToInt256(uint8) 127 => 127
//@[size] run-call: uint8ToInt256(uint8) 127 => 127
//@[none] run-call: uint24ToInt256(uint24) 16777215 => -1
//@[gas] run-call: uint24ToInt256(uint24) 16777215 => -1
//@[size] run-call: uint24ToInt256(uint24) 16777215 => -1
//@[none] run-call: narrowInt24(int24) -129 => 127
//@[gas] run-call: narrowInt24(int24) -129 => 127
//@[size] run-call: narrowInt24(int24) -129 => 127
//@[none] run-call: narrowInt24(int24) 128 => -128
//@[gas] run-call: narrowInt24(int24) 128 => -128
//@[size] run-call: narrowInt24(int24) 128 => -128
//@[none] run-call: directXor(int8,uint24) -1, 256 => 511
//@[gas] run-call: directXor(int8,uint24) -1, 256 => 511
//@[size] run-call: directXor(int8,uint24) -1, 256 => 511
//@[none] run-call: isUint8Max(int8) -1 => true
//@[gas] run-call: isUint8Max(int8) -1 => true
//@[size] run-call: isUint8Max(int8) -1 => true
//@[none] run-call: isUint8Max(int8) 1 => false
//@[gas] run-call: isUint8Max(int8) 1 => false
//@[size] run-call: isUint8Max(int8) 1 => false
//@[none] run-call: fullWidth(int256) -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call: fullWidth(int256) -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[size] run-call: fullWidth(int256) -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

contract IntegerConversions {
    function int8ToUint256(int8 value) external pure returns (uint256) {
        return uint256(uint8(value));
    }

    function int24ToUint256(int24 value) external pure returns (uint256) {
        return uint256(uint24(value));
    }

    function uint8ToInt256(uint8 value) external pure returns (int256) {
        return int256(int8(value));
    }

    function uint24ToInt256(uint24 value) external pure returns (int256) {
        return int256(int24(value));
    }

    function narrowInt24(int24 value) external pure returns (int256) {
        return int256(int8(value));
    }

    function directXor(int8 value, uint24 other) external pure returns (uint256) {
        return uint256(other) ^ uint256(uint8(value));
    }

    function isUint8Max(int8 value) external pure returns (bool) {
        return uint8(value) == type(uint8).max;
    }

    function fullWidth(int256 value) external pure returns (uint256) {
        return uint256(value);
    }
}
