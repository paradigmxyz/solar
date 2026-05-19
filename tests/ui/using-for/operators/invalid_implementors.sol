//@compile-flags: -Ztypeck
// Ported from test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_contract_function_at_file_level.sol.
// Ported from test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_library_function_at_file_level.sol.

type U is uint256;

contract C {
    event E(U, U);

    function add(U a, U b) public pure returns (U) {
        return U.wrap(U.unwrap(a) + U.unwrap(b));
    }
}

library L {
    function add(U a, U b) internal pure returns (U) {
        return U.wrap(U.unwrap(a) + U.unwrap(b));
    }
}

using {C.add as +} for U global; //~ ERROR: only file-level functions and library functions
//~^ ERROR: only pure free functions can be used to define operators
using {L.add as -} for U global; //~ ERROR: only pure free functions can be used to define operators
