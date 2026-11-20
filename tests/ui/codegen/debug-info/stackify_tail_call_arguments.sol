//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// A call in tail position and the `return` after it are one tail call, as in LLVM's `TCRETURN`,
// which keeps the call's location. The pushes and swaps that set up its arguments map to the call
// `h(x, true)`, like its jump, not to the `return` statement around it. `-Onone` folds no call into
// a tail call, and the functions are tagged so that no inlining removes them at `-Ogas`.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke=g
// CHECK-NEXT: push 1 !metadata(span=[[CALL:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[CALL]])
// CHECK-NEXT: swap 1 !metadata(span=[[CALL]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[CALL]], exit=return)
contract C {
    /// @custom:solar-inline never
    function g(uint256 x) internal pure returns (uint256) {
        return h(x, true);
    }

    /// @custom:solar-inline never
    function h(uint256 x, bool f) internal pure returns (uint256) {
        return f ? x : 0;
    }

    function a(uint256 y) external pure returns (uint256) {
        return g(y);
    }
}
