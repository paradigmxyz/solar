// A typed shift by a negative or signed amount is a type error, which an array length that reads it
// reports as an unsupported operation.
uint8 constant A = 1;
uint256 constant X = A << -1; //~ ERROR: cannot apply builtin operator `<<` to `uint8` and `int_literal[1]`
int8 constant N = -1;
uint256 constant Y = A << N; //~ ERROR: cannot apply builtin operator `<<` to `uint8` and `int8`

contract C {
    uint256[X] x; //~ ERROR: failed to evaluate constant: unsupported binary operation
    uint256[Y] y; //~ ERROR: failed to evaluate constant: unsupported binary operation
}
