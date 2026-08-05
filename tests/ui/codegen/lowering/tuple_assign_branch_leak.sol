//@ compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:
// A multi-return tuple assignment inside one branch arm must not leak its
// values into the sibling arm: `off` below is reassigned only in the `then`
// arm, so the `else` arm must read the pre-branch value, not the pickup from
// the other arm's call. Debug builds validate use reachability, so the
// regression compiles only when the lowering marks tuple targets as assigned.
contract TupleAssignBranchLeak {
    function pair(uint256 x) internal pure returns (uint256, uint256) {
        return (x + 1, x + 2);
    }

    // CHECK: push 0x2143aa9
    // CHECK: eq
    // CHECK: calldataload
    // CHECK: jumpi
    // CHECK: push 1
    // CHECK: add
    // CHECK: push 2
    // CHECK: add
    // The else arm calls `pair(off + 7)` with the pre-branch `off`.
    // CHECK: push 448
    // CHECK: mload
    // CHECK: push 7
    // CHECK: dup2
    // CHECK: add
    // CHECK: jumpi
    function run(bool takeFirst, uint256 seed) external pure returns (uint256 out) {
        uint256 a = seed;
        uint256 off = seed;
        if (takeFirst) {
            (a, off) = pair(seed);
            out = a + off;
        } else {
            (uint256 b, uint256 c) = pair(off + 7);
            out = b + c + off;
        }
    }
}
