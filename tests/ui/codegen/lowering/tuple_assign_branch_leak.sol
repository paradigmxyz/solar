//@ revisions: ir run
//@[ir] compile-flags: -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: run true, 10 => 23
//@ run-call: run false, 10 => 47
// A multi-return tuple assignment inside one branch arm must not leak its
// values into the sibling arm: `off` below is reassigned only in the `then`
// arm, so the `else` arm must read the pre-branch value, not the pickup from
// the other arm's call. Debug builds validate use reachability, so the
// regression compiles only when the lowering marks tuple targets as assigned.
contract TupleAssignBranchLeak {
    function pair(uint256 x) internal pure returns (uint256, uint256) {
        return (x + 1, x + 2);
    }

    // CHECK-LABEL: @module TupleAssignBranchLeak_runtime
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push [[THEN:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // The fallthrough else arm adds to the pre-branch `off`, not the other arm's result.
    // CHECK-NEXT: push 7
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
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
