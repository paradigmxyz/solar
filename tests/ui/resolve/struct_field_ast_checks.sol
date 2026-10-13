// The AST validator checks struct field types too.

contract C {
    struct S {
        function() external returns (uint x) f; //~ ERROR: return parameters in function types may not be named
        //~^ WARN: named function type parameters are deprecated
        uint[1__0] a; //~ ERROR: invalid use of underscores in number literal
    }
}
