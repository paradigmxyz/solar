//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@ compile-flags: --emit=srcmap-runtime -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Stackify emits the edge from a branch to the join after a bypassed arm as a fallthrough when
// the join comes next. Block layout then moves the join away and the fallthrough becomes a real
// jump, which keeps the origin recorded for the fallthrough. That jump leaves an `if` arm, so it
// has no location, and the origin of the condition does not come back after the arm ran.
contract C {
    // CHECK-LABEL: invoke=g@
    // CHECK: mul !metadata(span=
    // CHECK: jumpi !metadata(span=
    // CHECK-NEXT: jump {{bb[0-9]+}}{{$}}
    function g(uint256 a) external pure returns (uint256 r) {
        if (a > 5) {
            r = a * 3;
        } else {
            r = a + 2;
        }
        for (uint256 i = 0; i < a; i++) {
            r += i;
        }
    }
}
