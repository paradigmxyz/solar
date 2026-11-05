// Like solc, only `0x` is a number prefix: a letter after a number is an illegal number end.

contract C {
    uint a = 0b; //~ ERROR: identifier-start is not allowed at end of a number
    uint b = 0b1; //~ ERROR: identifier-start is not allowed at end of a number
    uint c = 0b2; //~ ERROR: identifier-start is not allowed at end of a number
    uint d = 0b_1; //~ ERROR: identifier-start is not allowed at end of a number
    uint e = 0b1_0; //~ ERROR: identifier-start is not allowed at end of a number
    uint f = 0o; //~ ERROR: identifier-start is not allowed at end of a number
    uint g = 0o7; //~ ERROR: identifier-start is not allowed at end of a number
    uint h = 0o8; //~ ERROR: identifier-start is not allowed at end of a number
    uint i = 0o_7; //~ ERROR: identifier-start is not allowed at end of a number
    uint j = 0B1; //~ ERROR: identifier-start is not allowed at end of a number
    uint k = 0O7; //~ ERROR: identifier-start is not allowed at end of a number
    uint l = 0X1; //~ ERROR: identifier-start is not allowed at end of a number
}
