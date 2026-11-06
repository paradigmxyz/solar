//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~[check]? ERROR: debugify: MIR module arrived with 3 instructions without a location
// The constructor that stages a library's deployment address has no source construct, so its
// instructions come out without an origin. LLVM's `not-generate` check reports them, and `check`
// rejects them.
library GapCheck {
    event Done(uint256 x);

    function f(uint256 x) external returns (uint256) {
        emit Done(x);
        return x + 1;
    }
}
