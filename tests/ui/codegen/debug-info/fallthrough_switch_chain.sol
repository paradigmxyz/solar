//@ compile-flags: --emit=bin -Onone -Zswitch-lowering=perfect -Zdebugify=report
//@ normalize-stderr-test: "(?s).*(debugify: EVM IR C after stackify: [^\n]*)\n.*" -> "$1"
//@ normalize-stdout-test: "(?s).+" -> ""

// The compare chain of a perfect-hash bucket falls through from each failed compare into the next
// bucket's label. The jump over that edge belongs to the dispatch switch.
contract C {
    uint256 s;

    function f0() external { s = 0; }
    function f1() external { s = 1; }
    function f2() external { s = 2; }
    function f3() external { s = 3; }
    function f4() external { s = 4; }
    function f5() external { s = 5; }
    function f6() external { s = 6; }
    function f7() external { s = 7; }
}
