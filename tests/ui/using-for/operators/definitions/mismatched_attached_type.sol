//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_and_returning_types_not_matching_using_for.sol

type Int is int256;

using {add as +, sub as -, div as /} for Int global;

function add(Int) pure returns (Int) {}
//~^ ERROR: wrong parameters
function sub(Int, Int, Int) pure returns (Int) {}
//~^ ERROR: wrong parameters
function div(int256, int256) pure returns (Int) {}
//~^ ERROR: wrong parameters
//~| ERROR: wrong return parameters

function f() pure {
    Int.wrap(0) + Int.wrap(1);
    //~^ ERROR: cannot apply builtin operator
    Int.wrap(0) - Int.wrap(0);
    //~^ ERROR: cannot apply builtin operator
    Int.wrap(0) / Int.wrap(0);
    //~^ ERROR: cannot apply builtin operator
}
