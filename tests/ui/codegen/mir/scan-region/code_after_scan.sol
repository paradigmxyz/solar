//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: log2Then 0, 0 => 0, 760652898201760790470484882029344460607866608383895
//@ run-call: log2Then 1, 1 => 0, 4684929948413084119870842297742379935417263717447575
//@ run-call: log2Then 10535140501722497024, 5 => 63, 477949036309978422790768084682429016121198829991831
//@ run-call: log2Then 57896044618658097711785492504343953926634992332820282019728792003956564819968, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 255, 689560553785340624572875971559537037065727270549399

// Straight-line code after a byte-table scan. Every class forks at the table, and its runs stop
// past the index rather than running the code after it, which would use up the work budget.
contract C {
    // CHECK-LABEL: fn @log2Then
    // CHECK: clz
    function log2Then(uint256 x, uint256 y) external pure returns (uint256 r, uint256 z) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
            z := y
            z := xor(mul(z, 3), add(z, 7))
            z := xor(mul(z, 4), add(z, 8))
            z := xor(mul(z, 5), add(z, 9))
            z := xor(mul(z, 6), add(z, 10))
            z := xor(mul(z, 7), add(z, 11))
            z := xor(mul(z, 8), add(z, 12))
            z := xor(mul(z, 9), add(z, 13))
            z := xor(mul(z, 10), add(z, 14))
            z := xor(mul(z, 11), add(z, 15))
            z := xor(mul(z, 12), add(z, 16))
            z := xor(mul(z, 13), add(z, 17))
            z := xor(mul(z, 14), add(z, 18))
            z := xor(mul(z, 15), add(z, 19))
            z := xor(mul(z, 16), add(z, 20))
            z := xor(mul(z, 17), add(z, 21))
            z := xor(mul(z, 18), add(z, 22))
            z := xor(mul(z, 19), add(z, 23))
            z := xor(mul(z, 20), add(z, 24))
            z := xor(mul(z, 21), add(z, 25))
            z := xor(mul(z, 22), add(z, 26))
            z := xor(mul(z, 23), add(z, 27))
            z := xor(mul(z, 24), add(z, 28))
            z := xor(mul(z, 25), add(z, 29))
            z := xor(mul(z, 26), add(z, 30))
            z := xor(mul(z, 27), add(z, 31))
            z := xor(mul(z, 28), add(z, 32))
            z := xor(mul(z, 29), add(z, 33))
            z := xor(mul(z, 30), add(z, 34))
            z := xor(mul(z, 31), add(z, 35))
            z := xor(mul(z, 32), add(z, 36))
            z := xor(mul(z, 33), add(z, 37))
            z := xor(mul(z, 34), add(z, 38))
            z := xor(mul(z, 35), add(z, 39))
            z := xor(mul(z, 36), add(z, 40))
            z := xor(mul(z, 37), add(z, 41))
            z := xor(mul(z, 38), add(z, 42))
            z := xor(mul(z, 39), add(z, 43))
            z := xor(mul(z, 40), add(z, 44))
            z := xor(mul(z, 41), add(z, 45))
            z := xor(mul(z, 42), add(z, 46))
        }
    }
}
