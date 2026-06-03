// Bare EVM builtins in inline assembly must be called with parentheses.
contract C {
    function f() external view returns (uint id, address a, uint g) {
        assembly {
            id := chainid //~ ERROR: builtin function `chainid` must be called
            a := caller //~ ERROR: builtin function `caller` must be called
            g := gas //~ ERROR: builtin function `gas` must be called
        }
    }
}
