// ported-from: test/libsolidity/syntaxTests/array/length/cyclic_constant.sol
// ported-from: test/libsolidity/syntaxTests/array/length/complex_cyclic_constant.sol

contract CyclicConstant {
    uint constant LEN = LEN;
    function f() public {
        uint[LEN] a; //~ ERROR: failed to evaluate constant: cyclic constant definition
    }
}

contract ComplexCyclicConstant {
    uint constant L2 = LEN - 10;
    uint constant L1 = L2 / 10;
    uint constant LEN = 10 + L1 * 5;
    function f() public {
        uint[LEN] a; //~ ERROR: failed to evaluate constant: cyclic constant definition
    }
}

// Like solc, array lengths follow at most 32 levels of constants defined by other constants.
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
    uint256[C32] tooDeep; //~ ERROR: failed to evaluate constant: constants are nested too deeply
    // solc evaluates each constant once, so `D` adds no depth after `C31` is evaluated.
    uint256[C31 + D] shared;
}

uint256 constant D = C31;

// The depth counts constants of every type, and constants that solc does not evaluate.
bytes32 constant B0 = B1;
bytes32 constant B1 = B2;
bytes32 constant B2 = B3;
bytes32 constant B3 = B4;
bytes32 constant B4 = B5;
bytes32 constant B5 = B6;
bytes32 constant B6 = B7;
bytes32 constant B7 = B8;
bytes32 constant B8 = B9;
bytes32 constant B9 = B10;
bytes32 constant B10 = B11;
bytes32 constant B11 = B12;
bytes32 constant B12 = B13;
bytes32 constant B13 = B14;
bytes32 constant B14 = B15;
bytes32 constant B15 = B16;
bytes32 constant B16 = B17;
bytes32 constant B17 = B18;
bytes32 constant B18 = B19;
bytes32 constant B19 = B20;
bytes32 constant B20 = B21;
bytes32 constant B21 = B22;
bytes32 constant B22 = B23;
bytes32 constant B23 = B24;
bytes32 constant B24 = B25;
bytes32 constant B25 = B26;
bytes32 constant B26 = B27;
bytes32 constant B27 = B28;
bytes32 constant B28 = B29;
bytes32 constant B29 = B30;
bytes32 constant B30 = B31;
bytes32 constant B31 = B32;
bytes32 constant B32 = B33;
bytes32 constant B33 = bytes32(uint256(1));
uint256 constant H = uint256(keccak256("x"));
uint256 constant U0 = U1;
uint256 constant U1 = U2;
uint256 constant U2 = U3;
uint256 constant U3 = U4;
uint256 constant U4 = U5;
uint256 constant U5 = U6;
uint256 constant U6 = U7;
uint256 constant U7 = U8;
uint256 constant U8 = U9;
uint256 constant U9 = U10;
uint256 constant U10 = U11;
uint256 constant U11 = U12;
uint256 constant U12 = U13;
uint256 constant U13 = U14;
uint256 constant U14 = U15;
uint256 constant U15 = U16;
uint256 constant U16 = U17;
uint256 constant U17 = U18;
uint256 constant U18 = U19;
uint256 constant U19 = U20;
uint256 constant U20 = U21;
uint256 constant U21 = U22;
uint256 constant U22 = U23;
uint256 constant U23 = U24;
uint256 constant U24 = U25;
uint256 constant U25 = U26;
uint256 constant U26 = U27;
uint256 constant U27 = U28;
uint256 constant U28 = U29;
uint256 constant U29 = U30;
uint256 constant U30 = U31;
uint256 constant U31 = U32;
uint256 constant U32 = U33;
uint256 constant U33 = H;

contract OtherTypes {
    uint256[uint256(B0)] bytesChain; //~ ERROR: failed to evaluate constant: constants are nested too deeply
    uint256[U0] unsupportedEnd; //~ ERROR: failed to evaluate constant: constants are nested too deeply
}
