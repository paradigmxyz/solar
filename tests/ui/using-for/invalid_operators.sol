//@compile-flags: -Ztypeck

type U is uint256;
type V is uint256;

function add(U a, U b) pure returns (U) {
    return a;
}

function add2(U a, U b) pure returns (U) {
    return b;
}

function badParams(uint256 a, uint256 b) pure returns (uint256) {
    //~^ ERROR: wrong parameters
    //~| ERROR: wrong return parameters
    return a + b;
}

function badReturn(U a, U b) pure returns (uint256) {
    //~^ ERROR: wrong return parameters
    return U.unwrap(a) + U.unwrap(b);
}

function notPure(U a, U b) view returns (U) {
    return a;
}

using {add as +, add2 as +} for U global;
//~^ ERROR: has more than one definition
//~| ERROR: has more than one definition
using {badParams as -} for U global;
using {badReturn as *} for U global;
using {notPure as /} for U global; //~ ERROR: only pure free functions

function vadd(V a, V b) pure returns (V) {
    return a;
}

contract C {
    using {vadd as +} for V; //~ ERROR: operators can only be defined in a global
}
