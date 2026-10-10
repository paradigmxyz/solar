// ported-from: test/libsolidity/syntaxTests/types/rational_number_div_limit.sol
// ported-from: test/libsolidity/syntaxTests/types/rational_number_mul_limit.sol

contract DivLimit {
    function f() public pure {
        int a;
        a = 1/(2<<4094)/(2<<4094); //~ ERROR: failed to evaluate constant: arithmetic overflow
    }
}

contract MulLimit {
    function f() public pure {
        int a;
        a = (1<<4095)*(1<<4095); //~ ERROR: failed to evaluate constant: arithmetic overflow
    }
}

// Like solc, literal arithmetic keeps 4096 bits of precision in the numerator and denominator.
// Intermediate values may exceed every integer type, as long as the result fits. Exponents are
// bounded by the bit length of the base, so `2 ** 2049` fails although it needs fewer bits.
contract Limits {
    uint[(1 << 4095) >> 4094] shiftFits;
    uint[((1 << 4095) * 2) >> 4095] productFits;
    uint[(1 << 4096) >> 4095] shiftTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[2 ** 2048 / 2 ** 2047] powFits;
    uint[2 ** 2049 / 2 ** 2048] powTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[0.5 ** 2048 * 2 ** 2048] fractionPowFits;
    uint[0.5 ** 2049 * 2 ** 2049] fractionPowTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    //~^ ERROR: failed to evaluate constant: arithmetic overflow
    uint[2 ** -2048 * 2 ** 2048] negativePowFits;
    uint[1 / (1 << 4095) * (1 << 4095)] denominatorFits;
    uint[1 / 2 ** 4095 * 2 ** 4095] powDenominatorTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    //~^ ERROR: failed to evaluate constant: arithmetic overflow
    uint[(1 << 300) >> 290] wide;
    uint[0 ** 4294967296 + 1 ** 4294967296 + (-1) ** 4294967297 + 2] trivialBases;
    uint[2 ** 4294967296] exponentTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[1 << 4294967296] shiftAmountTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[1 >> 4294967296] rightShiftAmountTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[0 << 4294967295 + 1] zeroShiftAmountTooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    // Unary operators keep any precision.
    uint[(~(((1 << 4095) * 2 - 1) * 2 + 1) >> 4096) + 3] bitNotWide;
}
