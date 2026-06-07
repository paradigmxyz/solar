// Bare EVM builtins in inline assembly must be called with parentheses.
contract C {
    function f() external view returns (uint id, address a, uint g) {
        assembly {
            id := chainid //~ ERROR: builtin function `chainid` must be called
            //~^ ERROR: unresolved symbol `chainid`
            a := caller //~ ERROR: builtin function `caller` must be called
            //~^ ERROR: unresolved symbol `caller`
            g := gas //~ ERROR: builtin function `gas` must be called
            //~^ ERROR: unresolved symbol `gas`
        }
    }
}
