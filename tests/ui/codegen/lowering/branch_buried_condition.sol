//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: merge [], [] => []
//@ run-call: merge [5], [] => [5]
//@ run-call: merge [], [6, 7] => [6, 7]
//@ run-call: merge [2, 2], [2] => [2, 2, 2]
//@ run-call: merge [1, 4, 9], [2, 3, 10, 11] => [1, 2, 3, 4, 9, 10, 11]
//@ run-call: merge [8, 9], [1, 2, 3] => [1, 2, 3, 8, 9]
//@ run-call: mergedLength [1, 4, 9], [2, 3, 10, 11] => 7

contract BranchBuriedCondition {
    function merge(uint256[] memory a, uint256[] memory b) external pure returns (uint256[] memory) {
        return _merge(a, b);
    }

    function mergedLength(uint256[] memory a, uint256[] memory b) external pure returns (uint256) {
        return _merge(a, b).length;
    }

    // Both elements are on the stack for the comparison, and the arm that takes `v` no longer
    // needs `u`. Dropping it after the bounds check of `c[k]` is computed brings `v` up over
    // that check's condition. The branch swaps the condition back to the top rather than drain
    // the stack to reach it, so the arm keeps its carried words (the two input pointers with
    // their ends, the output pointer, its index and length) and its only memory access is
    // the store.
    // CHECK-LABEL: @module BranchBuriedCondition_runtime
    // CHECK: [loop]:
    // CHECK: gt
    // CHECK-NEXT: push [[TAKE_V:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[TAKE_V]] [loop]:
    // CHECK-NEXT: dup 10
    // CHECK-NEXT: dup 4
    // CHECK-NEXT: lt
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: mstore
    // CHECK-NOT: mload
    // CHECK: jump
    function _merge(uint256[] memory a, uint256[] memory b)
        private
        pure
        returns (uint256[] memory c)
    {
        c = new uint256[](a.length + b.length);
        uint256 i;
        uint256 j;
        uint256 k;
        while (i < a.length && j < b.length) {
            uint256 u = a[i];
            uint256 v = b[j];
            if (u > v) {
                c[k] = v;
                ++j;
            } else {
                c[k] = u;
                ++i;
            }
            ++k;
        }
        while (i < a.length) c[k++] = a[i++];
        while (j < b.length) c[k++] = b[j++];
    }
}
