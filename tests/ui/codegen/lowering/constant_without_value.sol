//@ compile-flags: --emit=bin
// A constant is its value, which constant evaluation computes once. solc evaluates the initializer
// at every use instead, which can revert there; we report a constant without a value at its first
// use, once. See CODEGEN-013 in docs/SOLC_DIVERGENCE.md.

contract D {}

contract C {
    uint256 constant MAX = type(uint256).max;
    uint256 constant OVERFLOW = MAX + 1; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint256 constant POW = 2 ** MAX; //~ ERROR: failed to evaluate constant: arithmetic overflow
    uint256 constant ZERO = 0;
    uint256 constant DIVIDED = MAX / ZERO; //~ ERROR: failed to evaluate constant: attempted to divide by zero
    // Both branches of a conditional evaluate.
    int8 constant BRANCH = true ? int8(1) : int8(-128) - 1; //~ ERROR: failed to evaluate constant: arithmetic overflow
    // Only codegen knows the bytecode, so its hash has no value.
    bytes32 constant CODE_HASH = keccak256(type(D).creationCode); //~ ERROR: failed to evaluate constant: unsupported expression
    // Unused constants are not reported.
    uint256 constant UNUSED = MAX + 2;

    function f() external pure returns (uint256, uint256, uint256) {
        OVERFLOW;
        (OVERFLOW, POW);
        return (C.OVERFLOW, DIVIDED, POW);
    }

    function g() external pure returns (int8, bytes32) {
        unchecked {
            return (BRANCH, CODE_HASH);
        }
    }

    // The bytecode of a contract is a value.
    bytes constant CODE = type(D).runtimeCode;

    function h() external pure returns (bytes memory) {
        return CODE;
    }
}
