//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[ir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: factorial 0 => 1
//@ run-call: factorial 10 => 3628800
//@ run-call-fail: factorial 58 => Panic(0x11)

contract ProductSlot {
    // The product takes the slot of the old result, which dies at the overflow
    // check right after, so the check needs no further swaps.
    // CHECK: mul
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: dup 4
    // CHECK-NEXT: div
    // CHECK-NEXT: sub
    function factorial(uint256 n) external pure returns (uint256 result) {
        result = 1;
        for (uint256 i = 2; i <= n; ++i) result *= i;
    }
}
