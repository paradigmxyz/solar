//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A call in tail position and the `return` after it are one tail call, as in LLVM's `TCRETURN`,
// which keeps the call's location. The pushes and swaps that set up its arguments map to the call
// `h(x, true)`, like its jump, not to the `return` statement around it.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke=g
// CHECK-NEXT: push 1 !metadata(span=[[CALL:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[CALL]])
// CHECK-NEXT: swap 1 !metadata(span=[[CALL]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[CALL]], exit=return)
contract C {
    function g(uint256 x) internal pure returns (uint256) {
        return h(x, true);
    }

    function h(uint256 x, bool f) internal pure returns (uint256) {
        return f ? x : 0;
    }

    function a() external pure returns (uint256) {
        return g(42);
    }
}
