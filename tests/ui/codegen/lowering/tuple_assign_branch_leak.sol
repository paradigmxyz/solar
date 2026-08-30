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

    // CHECK: push 0x2143aa9
    // CHECK: eq
    // The else arm computes `pair(off + 7)` from the pre-branch `off`.
    // CHECK: push 160
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 192
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push [[THEN:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 192
    // CHECK-NEXT: mload
    // CHECK: push 7
    // CHECK-NEXT: dup 2
    // CHECK: add
    // CHECK: lt
    // CHECK-NEXT: push [[OVERFLOW:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push [[ELSE_RET:bb[0-9]+]]
    // CHECK-NEXT: push 224
    // CHECK-NEXT: mload
    // CHECK-NEXT: jump [[PAIR:bb[0-9]+]]
    // CHECK: [[PAIR]]:
    // CHECK: push 1
    // CHECK: add
    // The then arm calls the same helper with `seed`.
    // CHECK: [[THEN]]:
    // CHECK-NEXT: push [[THEN_RET:bb[0-9]+]]
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: jump [[PAIR]]
    // The two-word return rotates the hidden return label over both results.
    // CHECK: push 2
    // CHECK: add
    // CHECK: swap 1
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: jump
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
