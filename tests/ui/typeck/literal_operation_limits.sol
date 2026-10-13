// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/112_exp_operator_exponent_too_big.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/370_shift_constant_left_excessive_rvalue.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/371_shift_constant_right_excessive_rvalue.sol

contract ExpOperatorExponentTooBig {
    function f() public returns (uint d) { return 2 ** 10000000000; } //~ ERROR: cannot apply builtin operator `**`
}

contract ShiftConstantLeftExcessiveRvalue {
    uint public a = 0x42 << 0x100000000; //~ ERROR: cannot apply builtin operator `<<`
}

contract ShiftConstantRightExcessiveRvalue {
    uint public a = 0x42 >> 0x100000000; //~ ERROR: cannot apply builtin operator `>>`
}

// Powers of `0`, `1` and `-1` are exact for any exponent, and shifts within 32 bits are exact when
// the result fits.
contract ExactValues {
    uint constant a = 0 ** 10000000000;
    uint constant b = 1 ** 10000000000;
    int constant c = (-1) ** 10000000001;
    uint constant d = 0 << 0xffffffff;
    uint constant e = 0x42 >> 0xffffffff;
    uint[1 ** 10000000000] f;
    uint[(0x42 >> 0xffffffff) + 1] g;
}
