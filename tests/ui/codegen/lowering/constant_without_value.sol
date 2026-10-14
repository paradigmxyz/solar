//@ compile-flags: --emit=bin
// A constant is its value, which constant evaluation computes once. solc evaluates the initializer
// at every use instead, which can revert there; we report a constant whose initializer reverts at
// its declaration, once, if code uses it. See CODEGEN-013 in docs/SOLC_DIVERGENCE.md.

contract D {}

contract C {
    uint256 constant MAX = type(uint256).max;
    uint256 constant OVERFLOW = MAX + 1; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint256 constant POW = 2 ** MAX; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint256 constant ZERO = 0;
    uint256 constant DIVIDED = MAX / ZERO; //~ ERROR: failed to evaluate constant: attempted to divide by zero
    // Both branches of a conditional evaluate.
    int8 constant BRANCH = true ? int8(1) : int8(-128) - 1; //~ ERROR: failed to evaluate constant: arithmetic overflow
    // Both operands of `&&` evaluate.
    bool constant AND = false && OVERFLOW > 0;
    // A revert in one operand wins over a value the evaluator does not compute in the other.
    uint256 constant HASH = uint256(keccak256(type(D).creationCode));
    uint8 constant MAX8 = 255;
    uint256 constant HASH_FIRST = HASH + (MAX8 + 1); //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint256 constant HASH_LAST = (MAX8 + 1) + HASH; //~ ERROR: failed to evaluate constant: arithmetic overflow
    // Unused constants are not reported.
    uint256 constant UNUSED = MAX + 2;

    function f() external pure returns (uint256, uint256, uint256) {
        OVERFLOW;
        (OVERFLOW, POW);
        return (C.OVERFLOW, DIVIDED, POW);
    }

    function h() external pure returns (uint256, uint256) {
        return (HASH_FIRST, HASH_LAST);
    }

    function g() external pure returns (int8, bool) {
        unchecked {
            return (BRANCH, AND);
        }
    }
}
