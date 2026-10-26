//@ codegen-matrix: standard
//@ run-call: widen8 0xabcdef80 => -128
//@ run-call: widen8 0xabcdef7f => 127
//@ run-call: widen24 0xdeadbeef800000 => -8388608
//@ run-call: widen24 0xdeadbeef7fffff => 8388607
//@ run-call: widen128 0xdead80000000000000000000000000000000 => -170141183460469231731687303715884105728
//@ run-call: widen128 0xdead7fffffffffffffffffffffffffffffff => 170141183460469231731687303715884105727
//@ run-call: widen24_scaled 0xdeadbeef800000 => -25165824
//@ run-call: widen24_scaled 0xdeadbeef7fffff => 25165821

// Assembly can leave dirty upper bits in a signed variable. Converting it to `int256` must sign
// extend from the declared width and ignore everything above it.
contract SextMask {
    function widen8(uint256 w) external pure returns (int256) {
        int8 t;
        assembly {
            t := w
        }
        return int256(t);
    }

    function widen24(uint256 w) external pure returns (int256) {
        int24 t;
        assembly {
            t := w
        }
        return int256(t);
    }

    function widen128(uint256 w) external pure returns (int256) {
        int128 t;
        assembly {
            t := w
        }
        return int256(t);
    }

    function widen24_scaled(uint256 w) external pure returns (int256) {
        int24 t;
        assembly {
            t := w
        }
        return int256(t) * 3;
    }
}
