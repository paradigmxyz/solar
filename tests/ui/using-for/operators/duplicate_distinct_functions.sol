//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/multiple_operator_definitions_different_functions_same_directive.sol

type Int is int256;

using {add as +} for Int global; //~ ERROR: has more than one definition
using {add2 as +} for Int global;
//~^ ERROR: has more than one definition
using {neg as -} for Int global; //~ ERROR: has more than one definition
using {neg2 as -} for Int global;
//~^ ERROR: has more than one definition

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function add2(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function neg(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}

function neg2(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}
