// The edge coverage instrumentation opcode is not a Yul builtin: only pass plugins emit it.

contract C {
    function f() public {
        assembly {
            edgecov(1) //~ ERROR: unresolved symbol `edgecov`
        }
    }
}
