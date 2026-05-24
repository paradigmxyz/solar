//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_and_returning_types_not_matching_using_for.sol

type U is uint256;

function badParams(uint256 a, uint256 b) pure returns (uint256) {
    //~^ ERROR: wrong parameters
    //~| ERROR: wrong return parameters
    return a + b;
}

function badReturn(U a, U b) pure returns (uint256) {
    //~^ ERROR: wrong return parameters
    return U.unwrap(a) + U.unwrap(b);
}

using {badParams as -} for U global;
using {badReturn as *} for U global;
