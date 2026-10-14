// An array length cannot read a constant computed from a type, such as `new S[](1)`, so a constant
// that reaches itself through the length of an array type in its initializer is an unsupported
// length, not a cycle the evaluation follows.
struct S {
    uint256[B] values; //~ ERROR: failed to evaluate constant: unsupported expression
}

uint256 constant A = uint256(keccak256(abi.encode(new S[](1)))) % 4 + 1;
uint256 constant B = A;

bytes constant X = abi.encode(new uint256[Y][](1)); //~ ERROR: failed to evaluate constant: unsupported expression
uint256 constant Y = X.length; //~ ERROR: initial value for constant variable has to be compile-time constant

contract C {
    uint256[Y] values; //~ ERROR: failed to evaluate constant: unsupported expression
}
