// Array lengths must evaluate to positive integers that fit in `uint256`, with solc's codes. Like
// solc, they only take number literals, constants of integer types and arithmetic on them.
enum E {
    A,
    B
}

int8 constant MINUS_ONE = -1;
uint8 constant ONE = 1;
uint256 constant MAX = 2 ** 256 - 1;

contract C {
    uint[7 / 2] fractional; //~ ERROR: array length cannot be fractional
    uint[E.B] enumValue; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[address(0)] addressValue; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[int8(-1)] negative; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[uint8(1) - 2] underflow; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[type(uint256).max + 1] tooLarge; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[keccak256("x")] hash; //~ ERROR: failed to evaluate constant: unsupported expression
    uint[MINUS_ONE] negativeConstant; //~ ERROR: array length cannot be negative
    uint[ONE - 2] underflowConstant; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint[MAX + 1] tooLargeConstant; //~ ERROR: failed to evaluate constant: arithmetic overflow
}

// A real error wins over a variable that has no value, in any order, as in solc.
contract Operands {
    function f(uint x) public pure {
        uint[x + (ONE - 2)] memory a; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint[(ONE - 2) + x] memory b; //~ ERROR: failed to evaluate constant: arithmetic overflow
        a;
        b;
    }
}

// Literal arithmetic is exact, so `(7 / 2) * 2` is 7.
contract Fractional {
    uint[(7 / 2) * 2] intermediate;
}
