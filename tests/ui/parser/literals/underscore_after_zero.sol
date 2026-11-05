// Like solc, a zero cannot be followed by an underscore, and `0x` needs a digit before any
// underscore.

contract C {
    uint a = 0_; //~ ERROR: identifier-start is not allowed at end of a number
    uint b = 0_1; //~ ERROR: identifier-start is not allowed at end of a number
    uint c = 0_0; //~ ERROR: identifier-start is not allowed at end of a number
    uint f = 0x; //~ ERROR: hexadecimal digit missing or invalid
    uint g = 0x_; //~ ERROR: hexadecimal digit missing or invalid
    uint h = 0x_1; //~ ERROR: hexadecimal digit missing or invalid
    uint i = 0x_a; //~ ERROR: hexadecimal digit missing or invalid
    uint j = 0x0X1; //~ ERROR: identifier-start is not allowed at end of a number
}
