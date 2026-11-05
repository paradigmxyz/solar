//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//@[report] filecheck:
//~[check]? ERROR: debugify: MIR pass `lower-abi` left 6 instructions without a location
// CHECK: GapCheck":{"bin":"{{[0-9a-f]+}}"}

// The check tests `x != 0` by jumping on `x`, and the ABI decoder it lowers has no source
// construct, so instructions come out without an origin. LLVM's `not-generate` check reports
// them, and `check` rejects them.
contract GapCheck {
    function f(uint256 x) external pure returns (uint256 r) {
        require(x != 0);
        r = 1;
    }
}
