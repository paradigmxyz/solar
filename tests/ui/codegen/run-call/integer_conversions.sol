//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: int8ToUint256(int8) -1 => 255
//@ run-call: int8ToUint256(int8) -128 => 128
//@ run-call: int8ToUint256(int8) 127 => 127
//@ run-call: int24ToUint256(int24) -1 => 16777215
//@ run-call: int24ToUint256(int24) -8388608 => 8388608
//@ run-call: int24ToUint256(int24) 8388607 => 8388607
//@ run-call: uint8ToInt256(uint8) 255 => -1
//@ run-call: uint8ToInt256(uint8) 128 => -128
//@ run-call: uint8ToInt256(uint8) 127 => 127
//@ run-call: uint24ToInt256(uint24) 16777215 => -1
//@ run-call: narrowInt24(int24) -129 => 127
//@ run-call: narrowInt24(int24) 128 => -128
//@ run-call: directXor(int8,uint24) -1, 256 => 511
//@ run-call: isUint8Max(int8) -1 => true
//@ run-call: isUint8Max(int8) 1 => false
//@ run-call: fullWidth(int256) -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

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
