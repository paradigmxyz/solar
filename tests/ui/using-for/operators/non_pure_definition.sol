//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_non_pure_function.sol

type U is uint256;

function notPure(U a, U b) view returns (U) {
    return a;
}

using {notPure as /} for U global; //~ ERROR: only pure free functions
