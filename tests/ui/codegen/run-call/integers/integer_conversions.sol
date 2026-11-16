//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: int8ToUint256 -1 => 255
//@ run-call: int8ToUint256 -128 => 128
//@ run-call: int8ToUint256 127 => 127
//@ run-call: int24ToUint256 -1 => 16777215
//@ run-call: int24ToUint256 -8388608 => 8388608
//@ run-call: int24ToUint256 8388607 => 8388607
//@ run-call: uint8ToInt256 255 => -1
//@ run-call: uint8ToInt256 128 => -128
//@ run-call: uint8ToInt256 127 => 127
//@ run-call: uint24ToInt256 16777215 => -1
//@ run-call: narrowInt24 -129 => 127
//@ run-call: narrowInt24 128 => -128
//@ run-call: directXor -1, 256 => 511
//@ run-call: isUint8Max -1 => true
//@ run-call: isUint8Max 1 => false
//@ run-call: fullWidth -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

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
