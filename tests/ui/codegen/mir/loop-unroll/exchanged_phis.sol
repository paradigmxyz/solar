//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
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

    function swaps(uint256 n, uint256 a, uint256 b) external pure returns (uint256) {
        for (uint256 i; i < n; ++i) {
            (a, b) = (b, a);
        }
        return a;
    }
}
