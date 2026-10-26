// Reaching the recursion limit while evaluating a deep constant must not hide the value of a
// shallower constant in the same chain from later checks.

contract C {
    uint256 constant C0 = 0;
    uint256 constant C1 = C0;
    uint256 constant C2 = C1;
    uint256 constant C3 = C2;
    uint256 constant C4 = C3;
    uint256 constant C5 = C4;
    uint256 constant C6 = C5;
    uint256 constant C7 = C6;
    uint256 constant C8 = C7;
    uint256 constant C9 = C8;
    uint256 constant C10 = C9;
    uint256 constant C11 = C10;
    uint256 constant C12 = C11;
    uint256 constant C13 = C12;
    uint256 constant C14 = C13;
    uint256 constant C15 = C14;
    uint256 constant C16 = C15;
    uint256 constant C17 = C16;
    uint256 constant C18 = C17;
    uint256 constant C19 = C18;
    uint256 constant C20 = C19;
    uint256 constant C21 = C20;
    uint256 constant C22 = C21;
    uint256 constant C23 = C22;
    uint256 constant C24 = C23;
    uint256 constant C25 = C24;
    uint256 constant C26 = C25;
    uint256 constant C27 = C26;
    uint256 constant C28 = C27;
    uint256 constant C29 = C28;
    uint256 constant C30 = C29;
    uint256 constant C31 = C30;
    uint256 constant C32 = C31;

    // The value of a shallower constant does not let a deep constant through the limit either.
    function g(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 m = addmod(a, b, C1); //~ ERROR: arithmetic modulo zero
        return 7 / C32 + m;
    }

    function f(uint256 x, uint256 a, uint256 b) public pure returns (uint256) {
        uint256 z = x / C32;
        return addmod(a, b, C1) + z; //~ ERROR: arithmetic modulo zero
    }
}
