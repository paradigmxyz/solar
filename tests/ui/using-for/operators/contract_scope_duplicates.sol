//@compile-flags: -Ztypeck

type Int is int256;

using {add as +} for Int global;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function add2(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

contract C {
    using {add2 as +} for Int; //~ ERROR: operators can only be defined in a global

    function f(Int a, Int b) public pure returns (Int) {
        return a + b;
    }
}
