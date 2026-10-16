//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: overflow() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: power() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: divided() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: discarded() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: discardedTuple() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: inUnchecked => 0
//@ run-call: branch => 1
//@ run-call: and => false
//@ run-call-fail: hashFirst() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: hashLast() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: unused => 1

// A constant whose initializer reverts has no value at compile time, so every use calls a helper
// that runs the initializer, as in solc. See CODEGEN-013 in docs/SOLC_DIVERGENCE.md.
contract D {}

contract C {
    uint256 constant MAX = type(uint256).max;
    uint256 constant OVERFLOW = MAX + 1;
    uint256 constant POW = 2 ** MAX;
    uint256 constant ZERO = 0;
    uint256 constant DIVIDED = MAX / ZERO;
    // The helper runs only the branch the conditional takes.
    int8 constant BRANCH = true ? int8(1) : int8(-128) - 1;
    // So does `&&`.
    bool constant AND = false && OVERFLOW > 0;
    uint256 constant HASH = uint256(keccak256(type(D).creationCode));
    uint8 constant MAX8 = 255;
    uint8 constant WRAPPED = MAX8 + 1;
    uint256 constant HASH_FIRST = HASH + (MAX8 + 1);
    uint256 constant HASH_LAST = (MAX8 + 1) + HASH;
    // A constant that no code uses is not an error, even if its initializer reverts.
    uint256 constant UNUSED = MAX + 2;

    // CHECK-LABEL: fn @overflow()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: {{v[0-9]+}} = icall @constant_
    function overflow() external pure returns (uint256) {
        return OVERFLOW;
    }

    function power() external pure returns (uint256) {
        return POW;
    }

    function divided() external pure returns (uint256) {
        return DIVIDED;
    }

    function discarded() external pure returns (uint256) {
        C.OVERFLOW;
        return 1;
    }

    // The tuple reads its elements in order.
    function discardedTuple() external pure returns (uint256) {
        (DIVIDED, OVERFLOW);
        return 1;
    }

    // A use inside `unchecked` calls a helper that wraps, like in solc.
    function inUnchecked() external pure returns (uint8) {
        unchecked {
            return WRAPPED;
        }
    }

    function branch() external pure returns (int8) {
        return BRANCH;
    }

    function and() external pure returns (bool) {
        return AND;
    }

    function hashFirst() external pure returns (uint256) {
        return HASH_FIRST;
    }

    function hashLast() external pure returns (uint256) {
        return HASH_LAST;
    }

    // CHECK-LABEL: fn @unused()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: ret 1
    function unused() external pure returns (uint256) {
        return 1;
    }
}
