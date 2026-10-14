// A constant that reaches itself through the length of an array type in its initializer is a
// cycle, which the evaluation of an array length or base slot reports.
struct S {
    uint256[B] values; //~ ERROR: failed to evaluate constant: cyclic constant definition
}

uint256 constant A = uint256(keccak256(abi.encode(new S[](1)))) % 4 + 1;
uint256 constant B = A;

bytes constant X = abi.encode(new uint256[Y][](1)); //~ ERROR: failed to evaluate constant: cyclic constant definition
//~^ ERROR: failed to evaluate constant: cyclic constant definition
uint256 constant Y = X.length; //~ ERROR: initial value for constant variable has to be compile-time constant

contract C {
    uint256[Y] values; //~ ERROR: failed to evaluate constant: cyclic constant definition
}
