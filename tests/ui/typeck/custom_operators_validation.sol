//@compile-flags: -Ztypeck

type Int is int128;

function impureAdd(Int x, Int y) view returns (Int) {}

function wrongArity(Int x) pure returns (Int) {
    return x;
}

function wrongArityUnary(Int x, Int y) pure returns (Int) {
    return x;
}

using {impureAdd as +} for Int global; //~ ERROR: only pure free functions can be used to define operators
using {wrongArity as *} for Int global; //~ ERROR: operator `*` cannot be unary
using {wrongArityUnary as ~} for Int global; //~ ERROR: operator `~` cannot be binary

enum E { A, B }
using {enumAdd as +} for E global; //~ ERROR: operators can only be implemented for user-defined value types

function enumAdd(E x, E y) pure returns (E) {
    return x;
}
