//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_returning_wrong_types.sol

type Int is int256;

using {add as +, div as /, unsub as -, bitnot as ~, gt as >, lt as <} for Int global;

function add(Int, Int) pure returns (int256) {}
//~^ ERROR: wrong return parameters
function div(Int, Int) pure {}
//~^ ERROR: wrong return parameters
function unsub(Int) pure returns (Int, Int) {}
//~^ ERROR: wrong return parameters
function bitnot(Int) pure returns (int256) {}
//~^ ERROR: wrong return parameters
function gt(Int, Int) pure returns (Int) {}
//~^ ERROR: wrong return parameters
function lt(Int, Int) pure returns (bool, Int) {}
//~^ ERROR: wrong return parameters

function f() pure {
    Int.wrap(0) + Int.wrap(1);
    Int.wrap(0) / Int.wrap(0);
    -Int.wrap(0);
    ~Int.wrap(0);
    Int.wrap(0) < Int.wrap(0);
    Int.wrap(0) > Int.wrap(0);
}
