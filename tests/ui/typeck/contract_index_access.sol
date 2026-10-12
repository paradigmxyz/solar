contract C {
    function f() view public { //~ WARN: function state mutability can be restricted to pure
        C[0];
    }
}
