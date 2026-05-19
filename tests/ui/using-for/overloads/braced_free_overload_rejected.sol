// Solc tests:
// - test/libsolidity/syntaxTests/using/free_functions_non_unique_err.sol.
// - test/libsolidity/syntaxTests/using/free_overloads.sol.

//@compile-flags: -Ztypeck

function f(uint8 x) pure returns (uint8) {
    return x;
}

function f(uint256 x) pure returns (uint256) {
    return x;
}

using {f} for uint256; //~ ERROR: expected function name
