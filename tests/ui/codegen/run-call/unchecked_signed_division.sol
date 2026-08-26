//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: uncheckedI8(int8,int8) -128, -1 => -128
//@ run-call: uncheckedI8(int8,int8) -128, 1 => -128
//@ run-call: uncheckedI8(int8,int8) -128, -2 => 64
//@ run-call: uncheckedI8(int8,int8) -127, -1 => 127
//@ run-call: uncheckedI8(int8,int8) -10, 3 => -3
//@ run-call: uncheckedI8(int8,int8) -10, -3 => 3
//@ run-call: uncheckedI24(int24,int24) -8388608, -1 => -8388608
//@ run-call: uncheckedI128(int128,int128) -170141183460469231731687303715884105728, -1 => -170141183460469231731687303715884105728
//@ run-call: uncheckedI256(int256,int256) -0x8000000000000000000000000000000000000000000000000000000000000000, -1 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: uncheckedI8(int8,int8) 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: checkedI8(int8,int8) -127, -1 => 127
//@ run-call-fail: checkedI8(int8,int8) -128, -1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: upcastI8(int8,int8) -128, -1 => -128
//@ run-call: internalI8(int8,int8) -128, -1 => -128
//@ run-call: isNegativeI8(int8,int8) -128, -1 => true

contract UncheckedSignedDivision {
    function uncheckedI8(int8 lhs, int8 rhs) external pure returns (int8) {
        unchecked {
            return lhs / rhs;
        }
    }

    function uncheckedI24(int24 lhs, int24 rhs) external pure returns (int24) {
        unchecked {
            return lhs / rhs;
        }
    }

    function uncheckedI128(int128 lhs, int128 rhs) external pure returns (int128) {
        unchecked {
            return lhs / rhs;
        }
    }

    function uncheckedI256(int256 lhs, int256 rhs) external pure returns (int256) {
        unchecked {
            return lhs / rhs;
        }
    }

    function checkedI8(int8 lhs, int8 rhs) external pure returns (int8) {
        return lhs / rhs;
    }

    function upcastI8(int8 lhs, int8 rhs) external pure returns (int256 result) {
        int8 quotient;
        unchecked {
            quotient = lhs / rhs;
        }
        result = quotient;
    }

    function internalI8(int8 lhs, int8 rhs) external pure returns (int8) {
        return divideI8(lhs, rhs);
    }

    function isNegativeI8(int8 lhs, int8 rhs) external pure returns (bool) {
        int8 quotient;
        unchecked {
            quotient = lhs / rhs;
        }
        return quotient < 0;
    }

    function divideI8(int8 lhs, int8 rhs) private pure returns (int8) {
        unchecked {
            return lhs / rhs;
        }
    }
}
