//@ revisions: report check
//@ compile-flags: --emit=bin -Onone
//@[report] compile-flags: -Zdebugify=report
//@[check] compile-flags: -Zdebugify=check
//~[check]? ERROR: debugify: EVM IR pass `stackify` lost 1 source origins
//@ normalize-stdout-test: "(?s).+" -> ""
// The check tests `x != 0` by jumping on `x`, so no instruction implements the condition and
// stack scheduling emits EVM IR without its origin. The `stackify` record reports that loss at the
// MIR to EVM IR boundary.
contract StackifyBoundary {
    function f(uint256 x) external pure returns (uint256 r) {
        require(x != 0);
        r = 1;
    }
}
