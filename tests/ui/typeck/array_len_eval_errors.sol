// Array lengths must evaluate to positive integers that fit in `uint256`, with solc's codes.
enum E {
    A,
    B
}

contract C {
    uint[7 / 2] fractional; //~ ERROR: failed to evaluate constant: the result is not an integer
    uint[E.B] enumValue; //~ ERROR: failed to evaluate constant: expected an integer
    uint[address(0)] addressValue; //~ ERROR: failed to evaluate constant: expected an integer
    uint[int8(-1)] negative; //~ ERROR: array length cannot be negative
    uint[uint8(1) - 2] underflow; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[type(uint256).max + 1] tooLarge; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[keccak256("x")] hash; //~ ERROR: failed to evaluate constant: expected an integer
}

// solc evaluates `(7 / 2) * 2` exactly to 7; see TYPECK-021.
contract Fractional {
    uint[(7 / 2) * 2] intermediate; //~ ERROR: failed to evaluate constant: the result is not an integer
}
