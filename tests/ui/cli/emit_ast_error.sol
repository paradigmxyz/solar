//@ compile-flags: --emit=ast

contract C {
    function f() public {
        undefined; //~ ERROR: unresolved symbol `undefined`
    }
}
