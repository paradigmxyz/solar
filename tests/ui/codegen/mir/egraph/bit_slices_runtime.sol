//@ codegen-matrix: standard
//@ run-call: chains 0 => 0, 0, 0
//@ run-call: chains 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffff000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: overflow 0x8000000000000000000000000000000000000000000000000000000000000000 => 0, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: overflow 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0, 0
//@ run-call: slices 0x123480 => 128, 0x123400, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
//@ run-call: slices 0x12347f => 127, 0x123400, 127, 127
//@ run-call: slices 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

//@ run-call: repeated 0x80, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
//@ run-call: repeated 0x8080, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8080
//@ run-call: repeated 0x8080, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x8080
//@ run-call: masked 0 => 0x12345678, 0x1200000000000000000000000000000000000000000000000000000000000000
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x12345678, 0x1200000000000000000000000000000000000000000000000000000000000000
//@ run-call: offsets 0 => 40
//@ run-call: offsets 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 40
//@ run-call: addressRoundtrip => true
//@ run-call: narrowRoundtrip 255, 0xffffffffffffffffffffffffffffffffffffffff => 127, 0, 0x7fffffffffffffffffffffffffffffffffffffff
//@ run-call: narrowRoundtrip 127, 0x7fffffffffffffffffffffffffffffffffffffff => 127, 0, 0x7fffffffffffffffffffffffffffffffffffffff
//@ run-call: narrowRoundtrip 128, 0x8000000000000000000000000000000000000000 => 0, 0, 0
//@ run-call: comparisons 0 => false, true, false, true
//@ run-call: comparisons 13 => false, false, false, false
//@ run-call: comparisons 14 => true, false, true, false
//@ run-call: comparisons 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, false, false, true
//@ run-call: comparisons 0x8000000000000000000000000000000000000000000000000000000000000000 => true, false, false, true
contract BitSlices {
    function narrowRoundtrip(uint8 x, uint160 y) external pure returns (uint8, uint8, uint160) {
        return (((x << 1) | 1) >> 1, ((x << 8) | 1) >> 8, ((y << 1) | 1) >> 1);
    }

    function comparisons(uint256 x) external pure returns (bool, bool, bool, bool) {
        return (13 < x, 13 > x, int256(13) < int256(x), int256(13) > int256(x));
    }

    function masked(uint256 x) external pure returns (uint256 low, uint256 high) {
        assembly {
            low := and(or(shl(32, x), 0x12345678), 0xffffffff)
            high := and(or(shr(8, x), shl(248, 0x12)), shl(248, 0xff))
        }
    }

    function offsets(uint256 x) external pure returns (uint256 result) {
        assembly {
            result := sub(add(x, 32), sub(x, 8))
        }
    }

    function addressRoundtrip() external view returns (bool result) {
        assembly {
            result := eq(shr(96, or(shl(96, address()), 7)), address())
        }
    }

    function repeated(uint256 x, uint256 index) external pure returns (uint256 result) {
        assembly {
            result := signextend(index, signextend(index, x))
        }
    }

    function chains(uint256 x) external pure returns (uint256 left, uint256 right, uint256 signed) {
        assembly {
            left := shl(16, shl(8, x))
            right := shr(16, shr(8, x))
            signed := sar(16, sar(8, x))
        }
    }

    function overflow(uint256 x) external pure returns (uint256 left, uint256 right, uint256 signed) {
        assembly {
            left := shl(not(0), shl(1, x))
            right := shr(1, shr(255, x))
            signed := sar(not(0), sar(1, x))
        }
    }

    function slices(uint256 x) external pure returns (uint256 low, uint256 high, uint256 signed, uint256 nested) {
        assembly {
            low := shr(248, shl(248, x))
            high := shl(8, shr(8, x))
            signed := sar(248, shl(248, x))
            nested := signextend(0, signextend(2, x))
        }
    }
}
