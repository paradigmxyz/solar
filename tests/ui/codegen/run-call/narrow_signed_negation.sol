//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: uncheckedI8(int8) -128 => -128
//@ run-call: uncheckedI8(int8) -127 => 127
//@ run-call: uncheckedI8(int8) 1 => -1
//@ run-call: uncheckedI8(int8) 0 => 0
//@ run-call: uncheckedI24(int24) -8388608 => -8388608
//@ run-call: uncheckedI24(int24) 1 => -1
//@ run-call: uncheckedI128(int128) -170141183460469231731687303715884105728 => -170141183460469231731687303715884105728
//@ run-call: uncheckedI128(int128) 1 => -1
//@ run-call: uncheckedI256(int256) -0x8000000000000000000000000000000000000000000000000000000000000000 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: checkedI8(int8) -127 => 127
//@ run-call-fail: checkedI8(int8) -128 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: upcastI8(int8) -128 => -128
//@ run-call: internalI8(int8) -128 => -128

contract NarrowSignedNegation {
    function uncheckedI8(int8 value) external pure returns (int8) {
        unchecked {
            return -value;
        }
    }

    function uncheckedI24(int24 value) external pure returns (int24) {
        unchecked {
            return -value;
        }
    }

    function uncheckedI128(int128 value) external pure returns (int128) {
        unchecked {
            return -value;
        }
    }

    function uncheckedI256(int256 value) external pure returns (int256) {
        unchecked {
            return -value;
        }
    }

    function checkedI8(int8 value) external pure returns (int8) {
        return -value;
    }

    function upcastI8(int8 value) external pure returns (int256 result) {
        int8 negated;
        unchecked {
            negated = -value;
        }
        result = negated;
    }

    function internalI8(int8 value) external pure returns (int8) {
        return negateI8(value);
    }

    function negateI8(int8 value) private pure returns (int8) {
        unchecked {
            return -value;
        }
    }
}
