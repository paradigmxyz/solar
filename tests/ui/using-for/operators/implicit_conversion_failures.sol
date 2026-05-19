// Solc test: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_with_implicit_conversion.sol.

//@compile-flags: -Ztypeck

type U is uint256;

using {add as +, neg as -} for U global;

function add(U a, U b) pure returns (U) {
    return U.wrap(U.unwrap(a) + U.unwrap(b));
}

function neg(U a) pure returns (U) {
    return U.wrap(0 - U.unwrap(a));
}

contract C {
    function fromBool(bool x, U y) public pure {
        U a = y + x; //~ ERROR: cannot apply builtin operator
        U b = x + y; //~ ERROR: cannot apply builtin operator
        U c = -x; //~ ERROR: cannot apply unary operator
        //~^ ERROR: mismatched types
    }

    function fromUint(uint32 x, U y) public pure {
        U a = y + x; //~ ERROR: cannot apply builtin operator
        U b = x + y; //~ ERROR: cannot apply builtin operator
        U c = -x; //~ ERROR: cannot apply unary operator
        //~^ ERROR: mismatched types
    }
}
