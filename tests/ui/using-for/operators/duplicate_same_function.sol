//@compile-flags: -Ztypeck

type Int is int256;

using {add as +, add as +} for Int global;
using {neg as -} for Int global;
using {neg as -} for Int global;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function neg(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}

function f(Int a, Int b) pure returns (Int, Int) {
    return (a + b, -a);
}
