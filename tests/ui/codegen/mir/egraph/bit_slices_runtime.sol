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
contract BitSlices {
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
