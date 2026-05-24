//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/free_functions_implicit_conversion_err.sol

function id8(uint8 x) pure returns (uint8) {
    return x;
}

contract C {
    using {id8} for uint256; //~ ERROR: cannot be attached
}
