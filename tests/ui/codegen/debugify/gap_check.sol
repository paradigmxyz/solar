//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//~[check]? ERROR: debugify: MIR pass `lower-abi` left 15 instructions without a location
// The calldata decoder that both `bytes` parameters share has no source construct, so its
// instructions come out without an origin. LLVM's `not-generate` check reports them, and `check`
// rejects them.
contract GapCheck {
    function f(bytes calldata a, bytes calldata b) external pure returns (uint256 r) {
        r = a.length + b.length;
    }
}
