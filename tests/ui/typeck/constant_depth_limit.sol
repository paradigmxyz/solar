// ported-from: test/libsolidity/syntaxTests/array/length/cyclic_constant.sol
// ported-from: test/libsolidity/syntaxTests/array/length/complex_cyclic_constant.sol

contract CyclicConstant {
    uint constant LEN = LEN;
    function f() public {
        uint[LEN] a; //~ ERROR: failed to evaluate constant: recursion limit reached
    }
}

contract ComplexCyclicConstant {
    uint constant L2 = LEN - 10;
    uint constant L1 = L2 / 10;
    uint constant LEN = 10 + L1 * 5;
    function f() public {
        uint[LEN] a; //~ ERROR: failed to evaluate constant: recursion limit reached
    }
}

// Like solc, constants may be defined by at most 32 levels of other constants.
uint256 constant C0 = 1;
uint256 constant C1 = C0 + 1;
uint256 constant C2 = C1 + 1;
uint256 constant C3 = C2 + 1;
uint256 constant C4 = C3 + 1;
uint256 constant C5 = C4 + 1;
uint256 constant C6 = C5 + 1;
uint256 constant C7 = C6 + 1;
uint256 constant C8 = C7 + 1;
uint256 constant C9 = C8 + 1;
uint256 constant C10 = C9 + 1;
uint256 constant C11 = C10 + 1;
uint256 constant C12 = C11 + 1;
uint256 constant C13 = C12 + 1;
uint256 constant C14 = C13 + 1;
uint256 constant C15 = C14 + 1;
uint256 constant C16 = C15 + 1;
uint256 constant C17 = C16 + 1;
uint256 constant C18 = C17 + 1;
uint256 constant C19 = C18 + 1;
uint256 constant C20 = C19 + 1;
uint256 constant C21 = C20 + 1;
uint256 constant C22 = C21 + 1;
uint256 constant C23 = C22 + 1;
uint256 constant C24 = C23 + 1;
uint256 constant C25 = C24 + 1;
uint256 constant C26 = C25 + 1;
uint256 constant C27 = C26 + 1;
uint256 constant C28 = C27 + 1;
uint256 constant C29 = C28 + 1;
uint256 constant C30 = C29 + 1;
uint256 constant C31 = C30 + 1;
uint256 constant C32 = C31 + 1;

contract Depth {
    uint256[C31] deepest;
    uint256[C32] tooDeep; //~ ERROR: failed to evaluate constant: recursion limit reached
}
