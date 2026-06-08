//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_non_pure_function.sol

type U is uint256;

function add(U a, U b) view returns (U) {
    return a;
}

function sub(U a, U b) returns (U) {
    return a;
}

function mul(U a, U b) payable returns (U) {
    return a;
}

using {add as +, sub as -, mul as *} for U global;
//~^ ERROR: only pure free functions
//~| ERROR: only pure free functions
//~| ERROR: only pure free functions
