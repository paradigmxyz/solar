//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/multiple_operator_definitions_on_file_and_contract_level.sol

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
    //~^ ERROR: user-defined binary operator `+` has more than one definition

    function f(Int a, Int b) public pure returns (Int) {
        return a + b; //~ ERROR: user-defined operator has more than one matching definition
    }
}
