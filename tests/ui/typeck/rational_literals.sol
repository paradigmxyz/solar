contract RationalConversions {
    uint constant HALF = 3 / 2; //~ ERROR: mismatched types
    uint small = 1 / 100; //~ ERROR: mismatched types
    uint decimal = 0.5; //~ ERROR: mismatched types
    uint halfMax = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff / 2; //~ ERROR: mismatched types
    uint tooBig = 2 ** 256; //~ ERROR: mismatched types

    function f(uint x) public pure returns (uint) {
        uint a = 3 / 2; //~ ERROR: mismatched types
        uint b = uint(3 / 2); //~ ERROR: invalid explicit type conversion
        uint c = (3 / 2) + x; //~ ERROR: cannot apply builtin operator
        uint d = (3 / 2) & 1; //~ ERROR: failed to evaluate constant: unsupported binary operation
        uint e = 2 ** (1 / 2); //~ ERROR: failed to evaluate constant: unsupported binary operation
        uint f = 1 / 0; //~ ERROR: failed to evaluate constant: attempted to divide by zero
        uint shift = (3 / 2) << 1; //~ ERROR: failed to evaluate constant: unsupported binary operation
        int invert = ~(3 / 2); //~ ERROR: failed to evaluate constant: unsupported unary operation
        uint exhausted = (2 ** 4096) / (2 ** 4095); //~ ERROR: failed to evaluate constant: arithmetic overflow
        bool comparison = (7 / 2) > 3; //~ ERROR: cannot apply builtin operator
        return 1 / 100; //~ ERROR: mismatched types
    }
}
