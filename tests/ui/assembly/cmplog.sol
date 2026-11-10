// The comparison operand log opcode is not a Yul builtin: only pass plugins emit it.

contract C {
    function f() public {
        assembly {
            cmplog(1, 2, 3) //~ ERROR: unresolved symbol `cmplog`
        }
    }
}
