//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: swaps 0, 1, 2 => 1
//@ run-call: swaps 1, 1, 2 => 2
//@ run-call: swaps 2, 1, 2 => 1
//@ run-call: swaps 3, 1, 2 => 2
//@ run-call: swaps 4, 1, 2 => 1
//@ run-call: swaps 5, 1, 2 => 2
//@ run-call: swaps 7, 1, 2 => 2
//@ run-call: swaps 8, 1, 2 => 1
//@ run-call: swaps 9, 1, 2 => 2

// A loop that swaps two values passes each header phi to the other through the
// latch. Unrolled by four, a copied header's phi maps to another copied
// header's phi, and its uses follow that chain to a value that survives: four
// swaps leave both values unchanged, and the remainder loop keeps the pair.
contract Swap {
    // CHECK-LABEL: fn @swaps()
    // CHECK: [[N:v[0-9]+]] = calldataload 4
    // CHECK-NEXT: [[FIRST:v[0-9]+]] = calldataload 36
    // CHECK-NEXT: [[SECOND:v[0-9]+]] = calldataload 68
    // CHECK: [[I:v[0-9]+]] = phi [{{bb[0-9]+}}: 0], [{{bb[0-9]+}}: {{v[0-9]+}}]
    // CHECK-NEXT: [[AHEAD:v[0-9]+]] = add [[I]], 3
    // CHECK-NEXT: lt [[AHEAD]], [[N]]
    // CHECK: [[A:v[0-9]+]] = phi [{{bb[0-9]+}}: [[FIRST]]], [{{bb[0-9]+}}: [[B:v[0-9]+]]]
    // CHECK: [[B]] = phi [{{bb[0-9]+}}: [[SECOND]]], [{{bb[0-9]+}}: [[A]]]
    function swaps(uint256 n, uint256 a, uint256 b) external pure returns (uint256) {
        for (uint256 i; i < n; ++i) {
            (a, b) = (b, a);
        }
        return a;
    }
}
