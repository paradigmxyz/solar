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
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 192
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push [[THEN:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // The else arm calls `pair(off + 7)` with the pre-branch `off`.
    // CHECK: push 192
    // CHECK-NEXT: mload
    // CHECK: push 7
    // CHECK-NEXT: dup2
    // CHECK: add
    // CHECK: jumpi
    // CHECK-NEXT: push [[ELSE_RET:bb[0-9]+]]
    // CHECK-NEXT: push 288
    // CHECK-NEXT: mload
    // CHECK-NEXT: jump [[PAIR:bb[0-9]+]]
    // CHECK: [[PAIR]]:
    // CHECK: push 1
    // CHECK: add
    // CHECK: push 2
    // CHECK: add
    // CHECK: [[THEN]]:
    // CHECK-NEXT: push [[THEN_RET:bb[0-9]+]]
    // CHECK: jump [[PAIR]]
    // CHECK: [[THEN_RET]]:
    // CHECK-NEXT: push [[THEN_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[PICKUP:bb[0-9]+]]
    // CHECK: [[PICKUP]]:
    // CHECK: [[ELSE_RET]]:
    // CHECK-NEXT: push [[ELSE_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[PICKUP]]
    // CHECK: [[THEN_CONT]]:
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    // The else continuation combines both call results with the original
    // `off` slot, rather than a value assigned only in the then arm.
    // CHECK: [[ELSE_CONT]]:
    // CHECK: push 192
    // CHECK-NEXT: mload
    // CHECK: add
    // CHECK: jump [[RETURN]]
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
