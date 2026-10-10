// ported-from: test/libsolidity/syntaxTests/events/internal_type.sol
struct S {
    S[] ss;
}

contract C {
    event E1(function() internal); //~ ERROR: types containing internal function pointers cannot be event parameter types
    event E2(S); //~ ERROR: recursive types cannot be event parameter types
}
