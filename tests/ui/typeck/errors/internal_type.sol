// ported-from: test/libsolidity/syntaxTests/errors/internal_type.sol
error E1(function() internal); //~ ERROR: types containing internal function pointers cannot be error parameter types
error E2(S); //~ ERROR: recursive types cannot be error parameter types

struct S {
    S[] ss;
}
