//@ codegen-matrix: standard
//@ run-call: scale32 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: scale32 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scale32 0 => 0
//@ run-call: lengthBytes 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: lengthBytes 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleTwice 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe1
//@ run-call-fail: scaleTwice 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: fits1 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: fits1 0x8000000000000000000000000000000000000000000000000000000000000000 => false
//@ run-call: fitsSwapped1 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: fitsSwapped1 0x8000000000000000000000000000000000000000000000000000000000000000 => false
//@ run-call: overflows1 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: overflows1 0x8000000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: overflowsSwapped1 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: overflowsSwapped1 0x8000000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: fits5 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: fits5 0x800000000000000000000000000000000000000000000000000000000000000 => false
//@ run-call: fitsSwapped5 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: fitsSwapped5 0x800000000000000000000000000000000000000000000000000000000000000 => false
//@ run-call: overflows5 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: overflows5 0x800000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: overflowsSwapped5 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false
//@ run-call: overflowsSwapped5 0x800000000000000000000000000000000000000000000000000000000000000 => true
//@ run-call: fits255 0x1 => true
//@ run-call: fits255 0x2 => false
//@ run-call: fitsSwapped255 0x1 => true
//@ run-call: fitsSwapped255 0x2 => false
//@ run-call: overflows255 0x1 => false
//@ run-call: overflows255 0x2 => true
//@ run-call: overflowsSwapped255 0x1 => false
//@ run-call: overflowsSwapped255 0x2 => true
//@ run-call: fits5 0 => true
//@ run-call: overflows5 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true

// The shift pair of a checked product by a power of two tests the high bits of its operand.
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

    function fits1(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(shr(1, shl(1, y)), y)
        }
    }

    function fitsSwapped1(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(y, shr(1, shl(1, y)))
        }
    }

    function overflows1(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(shr(1, shl(1, y)), y))
        }
    }

    function overflowsSwapped1(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(y, shr(1, shl(1, y))))
        }
    }

    function fits5(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(shr(5, shl(5, y)), y)
        }
    }

    function fitsSwapped5(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(y, shr(5, shl(5, y)))
        }
    }

    function overflows5(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(shr(5, shl(5, y)), y))
        }
    }

    function overflowsSwapped5(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(y, shr(5, shl(5, y))))
        }
    }

    function fits255(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(shr(255, shl(255, y)), y)
        }
    }

    function fitsSwapped255(uint256 y) external pure returns (bool r) {
        assembly {
            r := eq(y, shr(255, shl(255, y)))
        }
    }

    function overflows255(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(shr(255, shl(255, y)), y))
        }
    }

    function overflowsSwapped255(uint256 y) external pure returns (bool r) {
        assembly {
            r := iszero(eq(y, shr(255, shl(255, y))))
        }
    }
}
