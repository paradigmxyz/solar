//@ codegen-matrix: standard
//@ run-call: arithmetic 32769, 65408 => 98177, 65409, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff007f, 0
//@ run-call: arithmetic 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0
//@ run-call: factored 0x55, 0xaa, 0xff => 0xff, 0xff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
//@ run-call: factored 0, 0, 0 => 0, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: packed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255, 255, 0
//@ run-call: packed 1 => 0, 0, 0
//@ run-call: shifts 1, 2, 0 => 3, 3
//@ run-call: shifts 1, 2, 255 => 0x8000000000000000000000000000000000000000000000000000000000000000, 0
//@ run-call: shifts 0x8000000000000000000000000000000000000000000000000000000000000000, 1, 256 => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: shifts 0x8000000000000000000000000000000000000000000000000000000000000000, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: boundary 0xffffffffffffffffffffffffffffffffffffffff => true
//@ run-call: boundary 0x10000000000000000000000000000000000000000 => false
//@ run-call: shared 1, 2 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc
//@ run-call-fail: checkedSum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
contract WordRecipes {
    function arithmetic(uint256 x, uint256 y) external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        assembly {
            a := add(and(x, y), or(x, y))
            b := add(and(x, y), xor(x, y))
            c := add(not(add(x, y)), x)
            d := add(sub(x, y), sub(y, x))
        }
    }

    function factored(uint256 x, uint256 y, uint256 mask) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := or(and(x, mask), and(y, mask))
            b := xor(and(x, mask), and(y, mask))
            c := and(not(x), not(y))
        }
    }

    function packed(uint256 x) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := and(shr(160, x), 255)
            b := and(shr(248, x), 255)
            c := and(shr(256, x), 255)
        }
    }

    function shifts(uint256 x, uint256 y, uint256 n) external pure returns (uint256 a, uint256 b) {
        assembly {
            a := or(shl(n, x), shl(n, y))
            b := xor(sar(n, x), sar(n, y))
        }
    }

    function boundary(uint256 x) external pure returns (bool) { return x < 1 << 160; }

    function shared(uint256 x, uint256 y) external pure returns (uint256 a, uint256 b, uint256 c) {
        a = ~x;
        b = ~y;
        c = a & b;
    }

    function checkedSum(uint256 x, uint256 y) external pure returns (uint256) {
        return (x & y) + (x | y);
    }
}
