//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/using_for_with_operator_at_contract_level_in_base_contract.sol

type Int is int256;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function add2(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

contract Base {
    using {add as +} for Int; //~ ERROR: operators can only be defined in a global

    function f(Int a, Int b) public pure returns (Int) {
        return a + b;
    }
}

contract Derived is Base {
    using {add2 as +} for Int; //~ ERROR: operators can only be defined in a global

    function g(Int a, Int b) public pure returns (Int) {
        return a + b;
    }
}

contract OnlyInherited is Base {
    function h(Int a, Int b) public pure returns (Int) {
        return a + b; //~ ERROR: cannot apply builtin operator `+`
    }
}
