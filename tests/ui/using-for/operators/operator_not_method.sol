//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_as_attached_function_via_function_name.sol

type U is uint256;

function add(U a, U b) pure returns (U) {
    return U.wrap(U.unwrap(a) + U.unwrap(b));
}

using {add as +} for U global;

contract C {
    function f(U a, U b) public pure returns (U) {
        U c = a + b;
        a.add(b); //~ ERROR: member `add` not found
        return c;
    }
}
