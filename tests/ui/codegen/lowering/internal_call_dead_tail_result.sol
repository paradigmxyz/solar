//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: contains [1, 3, 5, 7], 5 => true
//@ run-call: contains [1, 3, 5, 7], 4 => false
//@ run-call: contains [], 9 => false
//@ run-call: position [1, 3, 5, 7], 5 => true, 2
//@ run-call: position [1, 3, 5, 7], 4 => false, 2
//@ run-call: position [], 9 => false, 0

// A caller that never reads a stack-returned tuple's tail drops those words
// after the call instead of staging them in the callee's return area. With
// no call site staging there, and `needle`, which the search keeps across its
// loop, read from calldata in the callee, the callee needs no frame words, so
// both entries start their heap at 0x80.
contract DeadTailResult {
    function search(uint256[] memory a, uint256 needle)
        internal
        pure
        returns (bool found, uint256 index)
    {
        uint256 l = 0;
        uint256 h = a.length;
        while (l < h) {
            uint256 m = (l + h) / 2;
            uint256 t = a[m];
            if (t == needle) return (true, m);
            if (t < needle) l = m + 1;
            else h = m;
        }
        return (false, l);
    }

    // CHECK-LABEL: @module DeadTailResult_runtime
    // CHECK: revert
    // CHECK: push 128
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: [continuation]:
    // CHECK-NEXT: push [[CONTAINS:bb[0-9]+]]
    // CHECK-NEXT: swap 1
    // CHECK: [[CONTAINS]] [continuation]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: iszero
    function contains(uint256[] memory a, uint256 needle) external pure returns (bool found) {
        (found,) = search(a, needle);
    }

    function position(uint256[] memory a, uint256 needle) external pure returns (bool, uint256) {
        return search(a, needle);
    }
}
