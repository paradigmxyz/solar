//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/global_for_non_user_defined.sol
// ported-from: test/libsolidity/syntaxTests/using/global_library_for_builtin.sol
// ported-from: test/libsolidity/syntaxTests/using/global_library_for_interface.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_contract.sol

contract C {}

abstract contract A {}

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

function idArray(uint256[] memory x) pure returns (uint256[] memory) {
    return x;
}

function idFn(function() internal returns (uint256) x) pure returns (function() internal returns (uint256)) {
    return x;
}

function addC(C x, C y) pure returns (C) {
    y;
    return x;
}

function addA(A x, A y) pure returns (A) {
    y;
    return x;
}

using {id} for uint256 global; //~ ERROR: can only use `global` with user-defined types
using {idArray} for uint256[] global; //~ ERROR: can only use `global` with user-defined types
using {idFn} for function() internal returns (uint256) global; //~ ERROR: can only use `global` with user-defined types
using {idC} for C global; //~ ERROR: can only use `global` with user-defined types
using {idL} for L global; //~ ERROR: can only use `global` with user-defined types
using {idI} for I global; //~ ERROR: can only use `global` with user-defined types
using {addC as +} for C global;
//~^ ERROR: can only use `global` with user-defined types
//~| ERROR: operators can only be implemented for user-defined value types
using {addA as +} for A global;
//~^ ERROR: can only use `global` with user-defined types
//~| ERROR: operators can only be implemented for user-defined value types
