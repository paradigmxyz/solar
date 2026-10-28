//@ revisions: none jit many
//@ compile-flags: -Ogas -Zdump=mir
//@[none] compile-flags: -Ztune=none
//@[jit] compile-flags: -Ztune=jit
//@[many] compile-flags: -Ztune=none --optimize-runs 10000
//@[none] filecheck: --check-prefix=NONE
//@[jit] filecheck: --check-prefix=JIT
//@[many] filecheck: --check-prefix=MANY
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: few 0 => 0
//@ run-call: few 5 => 30
//@ run-call: few 7 => 63

// At the default 200 runs loops stay rolled. Under `-Ztune=jit`, a JIT compiles the copies of an
// unrolled loop to straight-line code, so the loop that carries two words, `i` and `sum`, unrolls
// at 200 runs. The loop that carries five words stays rolled, because a JIT keeps a word in four
// registers and spills the words of a wider loop to the stack on every iteration. Each unrolled
// loop adds a main loop whose header has the same phis as the original: with `many` as the
// reference, counting phis shows which loops unrolled (`few` 2 + `many` 5 = 7, `few` unrolled
// 2 + 2 + 5 = 9, both unrolled 2 + 2 + 5 + 5 = 14).
// NONE-COUNT-7: = phi
// NONE-NOT: phi
// JIT-COUNT-9: = phi
// JIT-NOT: phi
// MANY-COUNT-14: = phi
// MANY-NOT: phi
contract JitLoops {
    function few(uint256 n) external pure returns (uint256 sum) {
        unchecked {
            for (uint256 i; i < n; ++i) sum += i * 3;
        }
    }

    function many(uint256 n) external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        unchecked {
            for (uint256 i; i < n; ++i) {
                a += i;
                b ^= a;
                c += b;
                d ^= c + i;
            }
        }
    }
}
