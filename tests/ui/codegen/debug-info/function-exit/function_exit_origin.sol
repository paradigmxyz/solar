//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@ compile-flags: --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A function that falls off the end of its body exits at the body's closing brace, as the epilogue
// of a clang function does. The definition's first line would send a debugger that steps by line
// back to the header after the last statement ran. The entry keeps the whole definition.
contract C {
    uint256 stored;
    uint256 other;

    modifier guard() {
        other += 1;
        _;
    }

    // CHECK-LABEL: ; function external_exit(
    // CHECK: sstore !metadata(span=
    // CHECK-NEXT: stop !metadata(span=[[#EXT_END:]]..
    // CHECK-SAME: [[#EXT_END+1]], exit=return)
    function external_exit(uint256 a) external {
        stored = a;
    }

    // CHECK-LABEL: ; function external_return(
    // CHECK: return !metadata(span=[[#RET_END:]]..
    // CHECK-SAME: [[#RET_END+1]], exit=return)
    function external_return(uint256 a) external pure returns (uint256 r) {
        r = a + 1;
    }

    // A second call keeps `internal_exit` from being inlined at `-Ogas`.
    function twice(uint256 a, uint256 b) external {
        internal_exit(a);
        internal_exit(b);
    }

    // The explicit `return` keeps its own location.
    // CHECK-LABEL: ; function explicit_return(
    // CHECK: return !metadata(span=[[#RET:]]..
    // CHECK-SAME: [[#RET+13]], exit=return)
    function explicit_return(uint256 a) external pure returns (uint256) {
        return a + 1;
    }

    // The exit of a function that has modifiers is the closing brace of its own body.
    // CHECK-LABEL: ; function modified(
    // CHECK: stop !metadata(span=[[#MOD_END:]]..
    // CHECK-SAME: [[#MOD_END+1]], exit=return)
    function modified(uint256 a) external guard {
        stored = a;
    }

    // CHECK-LABEL: ; function internal_exit(
    // CHECK: jump !metadata(span=[[#INT_END:]]..
    // CHECK-SAME: [[#INT_END+1]], exit=return)
    function internal_exit(uint256 a) internal {
        if (a > 5) {
            stored = a;
            other = a + 1;
        }
        stored += a * 3;
        other += stored / (a + 1);
    }
}
