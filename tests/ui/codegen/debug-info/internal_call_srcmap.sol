//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Of the two calls to `g`, one enters it by a jump, which is `i`, and one by falling into it. The
// return jump is `o`. The dispatcher's `JUMPI`s that enter `f` and `h` are no calls.
// CHECK-LABEL: "srcmap-runtime"
// CHECK-NOT: :i
// CHECK: :o
// CHECK-NOT: :o
// CHECK: :i
// CHECK-NOT: :i
contract C {
    function g(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }

    function f(uint256 a) external pure returns (uint256) {
        return g(a) + g(a + 1);
    }

    function h(uint256 a) external pure returns (uint256) {
        return a + 1;
    }
}
