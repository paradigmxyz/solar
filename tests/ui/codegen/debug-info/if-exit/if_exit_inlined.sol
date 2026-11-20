//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Inlining gives code that the callee generated for itself the location of the call. The jump out
// of an `if` arm has no location by design, not for lack of one, so it keeps none in the caller.
contract C {
    uint256 stored;
    uint256 other;

    function helper(uint256 x) internal {
        if (x > 5) {
            stored = x;
            other = x + 1;
        }
    }

    // CHECK-LABEL: ; function f(
    // CHECK: sstore
    // CHECK: sstore
    // CHECK-NEXT: jump {{bb[0-9]+}}{{$}}
    function f(uint256 x) external {
        helper(x);
    }
}
