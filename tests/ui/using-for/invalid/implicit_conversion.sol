//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/free_functions_implicit_conversion_err.sol

struct S {
    uint8 x;
}

function id(uint16 x) pure returns (uint16) {
    return x;
}

contract C {
    using {id} for uint256; //~ ERROR: cannot be attached
    using {id} for S; //~ ERROR: cannot be attached
}
