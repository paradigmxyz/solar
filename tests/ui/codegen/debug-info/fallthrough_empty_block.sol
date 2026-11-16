//@ compile-flags: --emit=bin -Onone -Zdebugify=report
//@ normalize-stderr-test: "(?s).*(debugify: EVM IR C after stackify: [^\n]*)\n.*" -> "$1"
//@ normalize-stdout-test: "(?s).+" -> ""

// The ABI decoder call in `f` ends its MIR block, so the continuation after it holds only the
// jump to the next block. The jump still carries the origin of the MIR terminator it implements.
contract C {
    function f(uint256[] memory a, uint256[] memory b) public pure returns (uint256) {
        return a.length + b.length;
    }
}
