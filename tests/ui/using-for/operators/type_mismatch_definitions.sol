// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_and_returning_types_not_matching_using_for.sol

type Int is int256;

function add(Int a) pure returns (Int) {
    //~^ ERROR: wrong parameters
    return a;
}

function sub(Int a, Int b, Int c) pure returns (Int) {
    //~^ ERROR: wrong parameters
    b; c;
    return a;
}

function div(int256 a, int256 b) pure returns (Int) {
    //~^ ERROR: wrong parameters
    return Int.wrap(a / b);
}

using {add as +, sub as -, div as /} for Int global;

function f() pure {
    Int.wrap(0) + Int.wrap(1); //~ ERROR: cannot apply builtin operator
    Int.wrap(0) - Int.wrap(0); //~ ERROR: cannot apply builtin operator
    Int.wrap(0) / Int.wrap(0); //~ ERROR: cannot apply builtin operator
}
