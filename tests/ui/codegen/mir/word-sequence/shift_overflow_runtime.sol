//@ codegen-matrix: standard
//@ run-call: scale32 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: scale32 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scale32 0 => 0
//@ run-call: lengthBytes 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: lengthBytes 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleTwice 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe1
//@ run-call-fail: scaleTwice 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: fits1 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, false, true, false
//@ run-call: fits1 0x8000000000000000000000000000000000000000000000000000000000000000 => false, true, false, true
//@ run-call: fits5 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, false, true, false
//@ run-call: fits5 0x800000000000000000000000000000000000000000000000000000000000000 => false, true, false, true
//@ run-call: fits255 0x1 => true, false, true, false
//@ run-call: fits255 0x2 => false, true, false, true
//@ run-call: fits5 0 => true, false, true, false
//@ run-call: fits5 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false, true, false, true

// The shift pair of a checked product by a power of two compares through one bound.
contract ShiftOverflowRuntime {
    function scale32(uint256 x) external pure returns (uint256) {
        return x * 32;
    }

    function lengthBytes(uint256 len) external pure returns (uint256) {
        return (len + 1) * 32;
    }

    function scaleTwice(uint256 x) external pure returns (uint256 p, uint256 q) {
        p = x * 32;
        q = p + 1;
    }

    function fits1(uint256 y) external pure returns (bool a, bool b, bool c, bool d) {
        assembly {
            a := eq(shr(1, shl(1, y)), y)
            b := iszero(eq(shr(1, shl(1, y)), y))
            c := eq(y, shr(1, shl(1, y)))
            d := iszero(eq(y, shr(1, shl(1, y))))
        }
    }

    function fits5(uint256 y) external pure returns (bool a, bool b, bool c, bool d) {
        assembly {
            a := eq(shr(5, shl(5, y)), y)
            b := iszero(eq(shr(5, shl(5, y)), y))
            c := eq(y, shr(5, shl(5, y)))
            d := iszero(eq(y, shr(5, shl(5, y))))
        }
    }

    function fits255(uint256 y) external pure returns (bool a, bool b, bool c, bool d) {
        assembly {
            a := eq(shr(255, shl(255, y)), y)
            b := iszero(eq(shr(255, shl(255, y)), y))
            c := eq(y, shr(255, shl(255, y)))
            d := iszero(eq(y, shr(255, shl(255, y))))
        }
    }
}
