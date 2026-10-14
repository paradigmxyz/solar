// Array lengths must evaluate to positive integers that fit in `uint256`, with solc's codes.
enum E {
    A,
    B
}

contract C {
    uint[7 / 2] fractional; //~ ERROR: array length cannot be fractional
    uint[E.B] enumValue; //~ ERROR: failed to evaluate constant: expected an integer
    uint[address(0)] addressValue; //~ ERROR: failed to evaluate constant: expected an integer
    uint[int8(-1)] negative; //~ ERROR: array length cannot be negative
    uint[uint8(1) - 2] underflow; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[type(uint256).max + 1] tooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[keccak256("x")] hash; //~ ERROR: failed to evaluate constant: expected an integer
}

// Literal arithmetic is exact, so `(7 / 2) * 2` is 7.
contract Fractional {
    uint[(7 / 2) * 2] intermediate;
}
