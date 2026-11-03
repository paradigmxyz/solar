// solc parses the callee of `emit` and `revert` and the argument of `type` as expressions, so an
// unresolved first segment is an undeclared identifier.
contract C {
    struct S {
        uint x;
    }

    function f() public {
        emit Missing(); //~ ERROR: unresolved symbol `Missing`
        revert Missing(); //~ ERROR: unresolved symbol `Missing`
        emit Missing.E(); //~ ERROR: unresolved symbol `Missing`
        revert Missing.Er(); //~ ERROR: unresolved symbol `Missing`
        type(Missing).max; //~ ERROR: unresolved symbol `Missing`
        type(Missing.A).max; //~ ERROR: unresolved symbol `Missing`
        type(C.Missing).max; //~ ERROR: unresolved symbol `Missing`
        type(S.x).max; //~ ERROR: `S` is a struct, which cannot be indexed in type paths
    }
}
