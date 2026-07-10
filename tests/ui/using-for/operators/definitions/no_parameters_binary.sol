//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_no_parameters_binary.sol

type Int is int256;

using {f as +} for Int global;
//~^ ERROR: does not have any parameters

function f() returns (Int) {
    return Int.wrap(0);
}
