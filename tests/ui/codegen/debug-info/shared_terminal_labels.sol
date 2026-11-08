//@ codegen-matrix: standard
//@ compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ run-call: pick(uint256) 0 => 7
//@ run-call: pick(uint256) 1 => 7
//@ run-call: pick(uint256) 2 => 8
// Both successful branches can share machine code, but neither owns its label exclusively.
contract SharedTerminalLabels {
    function pick(uint256 x) external pure returns (uint256) {
        if (x == 0) return 7;
        if (x == 1) return 7;
        return 8;
    }
}
