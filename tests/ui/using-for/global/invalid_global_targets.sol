// Solc tests:
// - test/libsolidity/syntaxTests/using/global_for_non_user_defined.sol.
// - test/libsolidity/syntaxTests/using/global_library_for_builtin.sol.
// - test/libsolidity/syntaxTests/using/global_library_for_interface.sol.
// - test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_contract.sol.

//@compile-flags: -Ztypeck

contract C {}

library L {}

interface I {}

function id(uint256 x) pure returns (uint256) {
    return x;
}

function idC(C x) pure returns (C) {
    return x;
}

function idL(L x) pure returns (L) {
    return x;
}

function idI(I x) pure returns (I) {
    return x;
}

using {id} for uint256 global; //~ ERROR: can only use `global` with user-defined types
using {idC} for C global; //~ ERROR: can only use `global` with user-defined types
using {idL} for L global; //~ ERROR: can only use `global` with user-defined types
using {idI} for I global; //~ ERROR: can only use `global` with user-defined types
