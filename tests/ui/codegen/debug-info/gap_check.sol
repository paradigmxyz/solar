//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~[check]? ERROR: debugify: MIR pass `lower-dispatch` left 8 instructions without a location
// The selector dispatch that `lower-dispatch` builds has no source construct, so its instructions
// come out without an origin. LLVM's `not-generate` check reports them, and `check` rejects them.
contract GapCheck {
    function f(uint256 x) external pure returns (uint256 r) {
        require(x != 0);
        r = 1;
    }
}
