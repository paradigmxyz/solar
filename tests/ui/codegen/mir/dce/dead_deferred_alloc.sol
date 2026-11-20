//@ revisions: evmir run
//@[evmir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[evmir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: copy 3, 4 => 304
//@[run] run-call: keep 4 => 5
// A memory struct that nothing reads loses its stores, and its static allocation goes with
// them. The copy of a storage struct then allocates its snapshot at the heap floor, as the first
// allocation of the call, so the entry sets no free memory pointer and the snapshot's overflow
// test folds away.

contract DeadDeferredAlloc {
    struct S {
        uint256 x;
        uint256 y;
    }

    S s;
    S t;

    // CHECK-LABEL: function copy(uint256 a, uint256 b)
    // CHECK-NOT: push 64
    // CHECK-NOT: mload
    // CHECK: sstore
    // CHECK-NOT: push 64
    // CHECK-NOT: mload
    // CHECK: return
    function copy(uint256 a, uint256 b) external returns (uint256) {
        t = S(a, b);
        s = t;
        return s.x * 100 + s.y;
    }

    function keep(uint256 a) external pure returns (uint256) {
        S memory m = S(a, 2);
        return a + 1;
    }
}
