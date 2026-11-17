//@ revisions: none gas size
//@ compile-flags: --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// The call `h(...)` is the whole body of `g`, so the stores that spill `g`'s arguments before the
// call run for the call, not for the `return` around it. LLVM lowers a call and the return after
// it to one `TCRETURN` that keeps the call's `DebugLoc`, and puts a spill at the location of its
// insertion point. The spills take the span of the call, like its jump. `g` and `h` call each
// other, so no optimization level inlines them.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke=g
// CHECK-NEXT: swap 16 !metadata(span=[[CALL:[0-9]+..[0-9]+]])
// CHECK-NEXT: push {{[0-9]+}} !metadata(span=[[CALL]])
// CHECK-NEXT: mstore !metadata(span=[[CALL]])
// CHECK: jump bb{{[0-9]+}} !metadata(span=[[CALL]], exit=return)
contract C {
    function g(uint256 p0, uint256 p1, uint256 p2, uint256 p3, uint256 p4, uint256 p5, uint256 p6, uint256 p7, uint256 p8, uint256 p9, uint256 p10, uint256 p11, uint256 p12, uint256 p13, uint256 p14, uint256 p15, uint256 p16) internal pure returns (uint256) {
        return h(p16, p15, p14, p13, p12, p11, p10, p9, p8, p7, p6, p5, p4, p3, p2, p1, p0);
    }

    function h(uint256 p0, uint256 p1, uint256 p2, uint256 p3, uint256 p4, uint256 p5, uint256 p6, uint256 p7, uint256 p8, uint256 p9, uint256 p10, uint256 p11, uint256 p12, uint256 p13, uint256 p14, uint256 p15, uint256 p16) internal pure returns (uint256) {
        if (p0 == 0) return p0 * 1 + p1 * 2 + p2 * 3 + p3 * 4 + p4 * 5 + p5 * 6 + p6 * 7 + p7 * 8 + p8 * 9 + p9 * 10 + p10 * 11 + p11 * 12 + p12 * 13 + p13 * 14 + p14 * 15 + p15 * 16 + p16 * 17;
        return g(p0 - 1, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11, p12, p13, p14, p15, p16);
    }

    function a(uint256 x) external pure returns (uint256) {
        return g(x + 0, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7, x + 8, x + 9, x + 10, x + 11, x + 12, x + 13, x + 14, x + 15, x + 16);
    }

    function b(uint256 x) external pure returns (uint256) {
        return g(x * 2, x * 3, x * 4, x * 5, x * 6, x * 7, x * 8, x * 9, x * 10, x * 11, x * 12, x * 13, x * 14, x * 15, x * 16, x * 17, x * 18);
    }
}
