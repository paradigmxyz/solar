//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/global_for_non_user_defined.sol
// ported-from: test/libsolidity/syntaxTests/using/global_library_for_builtin.sol
// ported-from: test/libsolidity/syntaxTests/using/global_library_for_interface.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_contract.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_enum.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_struct.sol

contract C {}

library L {}

interface I {}

enum E {
    A,
    B
}

struct S {
    uint256 x;
}

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

function addE(E a, E b) pure returns (E) {
    b;
    return a;
}

function addS(S memory a, S memory b) pure returns (S memory) {
    b;
    return a;
}

using {id} for uint256 global; //~ ERROR: can only use `global` with user-defined types
using {idC} for C global; //~ ERROR: can only use `global` with user-defined types
using {idL} for L global; //~ ERROR: can only use `global` with user-defined types
using {idI} for I global; //~ ERROR: can only use `global` with user-defined types
using {addE as +} for E global; //~ ERROR: operators can only be implemented for user-defined value types
using {addS as +} for S global; //~ ERROR: operators can only be implemented for user-defined value types
