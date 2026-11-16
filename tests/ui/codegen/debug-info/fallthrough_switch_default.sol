//@ compile-flags: --emit=bin -Ogas -Zdebugify=report
//@ normalize-stderr-test: "(?s).*(debugify: EVM IR C after stackify: [^\n]*)\n.*" -> "$1"
//@ normalize-stdout-test: "(?s).+" -> ""

// A switch whose default is the next block cleans the stack and falls into it. The jump over that
// edge belongs to the switch, although stack scheduling has left the switch's block by then.
contract C {
    function switchOnly(uint256 a) internal pure returns (uint256 out) {
        uint256 twice = a + a;
        assembly {
            switch a
            case 0 { out := 1 }
            default { out := a }
        }
        return twice + out;
    }

    function i(uint256 x) external pure returns (uint256) { return switchOnly(x); }
    function j(uint256 x) external pure returns (uint256) { return switchOnly(x) + 1; }
}
