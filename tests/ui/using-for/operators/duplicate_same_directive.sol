//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/multiple_operator_definitions_different_functions_same_directive.sol

type U is uint256;

function add(U a, U b) pure returns (U) {
    return a;
}

function add2(U a, U b) pure returns (U) {
    return b;
}

using {add as +, add2 as +} for U global;
//~^ ERROR: has more than one definition
//~| ERROR: has more than one definition
