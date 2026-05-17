//@compile-flags: -Ztypeck

uint256 constant X = 1;

function id256(uint256 x) pure returns (uint256) {
    return x;
}

contract C {
    using id256 for uint256; //~ ERROR: expected library
    using {X} for uint256; //~ ERROR: expected function name
}
