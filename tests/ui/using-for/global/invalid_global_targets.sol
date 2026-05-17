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
