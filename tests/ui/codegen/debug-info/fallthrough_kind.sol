//@ compile-flags: --emit=bin -Onone -Zdebugify=report
//@ normalize-stderr-test: "(?s).*(debugify: EVM IR C after stackify: [^\n]*)\n.*" -> "$1"
//@ normalize-stdout-test: "(?s).+" -> ""

// The branches of the dispatch and of `f` have no origin, so the MIR audit marks them `Unknown`.
// A jump that falls through to the next block keeps that kind, so stack scheduling leaves no gap.
contract C {
    function f(uint256 x) external pure returns (uint256 r) {
        if (x != 0) {
            r = 1;
        }
    }
}
