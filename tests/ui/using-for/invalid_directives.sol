//@compile-flags: -Ztypeck
// Ported from test/libsolidity/syntaxTests/using/using_contract_err.sol.
// Ported from test/libsolidity/syntaxTests/using/using_free_no_parameters_err.sol.
// Ported from test/libsolidity/syntaxTests/using/free_functions_implicit_conversion_err.sol.
// Ported from test/libsolidity/syntaxTests/using/private_library_function_outside_scope.sol.
// Ported from test/libsolidity/syntaxTests/using/global_for_type_defined_elsewhere.sol.

uint256 constant X = 1;

function zero() pure returns (uint256) {
    return 0;
}

function id8(uint8 x) pure returns (uint8) {
    return x;
}

function id256(uint256 x) pure returns (uint256) {
    return x;
}

contract NotLibrary {
    function f(uint256 x) public pure returns (uint256) {
        return x;
    }
}

library L {
    struct Inner {
        uint256 x;
    }

    function priv(uint256 x) private pure returns (uint256) {
        return x;
    }
}

using {id256} for uint256 global; //~ ERROR: can only use `global` with user-defined types
using {id256} for L.Inner global; //~ ERROR: can only use `global` with types defined in the same source unit at file level
//~^ ERROR: cannot be attached

contract C {
    using NotLibrary for uint256; //~ ERROR: library name expected
    using {zero} for uint256; //~ ERROR: does not have any parameters
    using {id8} for uint256; //~ ERROR: cannot be attached
    using {L.priv} for uint256; //~ ERROR: is private and therefore cannot be attached
}
