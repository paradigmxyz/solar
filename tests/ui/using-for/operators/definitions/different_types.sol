//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_or_returning_different_types.sol

type Int is int128;

using {add as +, sub as -, mul as *, div as /} for Int global;

function add(Int, int128) pure returns (Int) {}
//~^ ERROR: wrong parameters
function sub(int128, Int) pure returns (int128) {}
//~^ ERROR: wrong parameters
//~| ERROR: wrong return parameters
function mul(int128, int256) pure returns (Int) {}
//~^ ERROR: wrong parameters
//~| ERROR: wrong return parameters
function div(Int, Int) pure returns (int256) {}
//~^ ERROR: wrong return parameters
