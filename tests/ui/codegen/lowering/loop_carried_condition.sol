//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: capacity 0 => 1
//@ run-call: capacity 1 => 2
//@ run-call: capacity 3 => 8
//@ run-call: capacity 8 => 16
//@ run-call-fail: capacity 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: capacities 3, 1 => 10

contract LoopCarriedCondition {
    function capacity(uint256 n) external pure returns (uint256) {
        return _capacity(n);
    }

    function capacities(uint256 a, uint256 b) external pure returns (uint256) {
        return _capacity(a) + _capacity(b);
    }

    // Doubling a word cannot be shown not to wrap, and the test for it is invariant, so
    // code motion leaves the header a branch on a word computed before the loop. The branch
    // does not compute that word, so `JUMPI` takes a copy and the word goes around the loop
    // on the stack with the bound beside it, where the latch finds both. Nothing in the loop
    // touches memory.
    // CHECK-LABEL: @module LoopCarriedCondition_runtime
    // CHECK: [[HEADER:bb[0-9]+]] [loop]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: push [[PANIC:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: push [[LATCH:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[LATCH]] [loop]:
    // CHECK-NOT: mload
    // CHECK: push [[HEADER]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: jump [[PANIC]]
    function _capacity(uint256 n) private pure returns (uint256 c) {
        c = 1;
        while (c < n * 2) c *= 2;
    }
}
