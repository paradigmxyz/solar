//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: accumulate 3, 0, 4 => 1
//@ run-call: accumulate 3, 7, 0 => 3
//@ run-call: accumulate 2, 3, 5 => 607
//@ run-call: accumulate 1, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: accumulate 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0x5555555555555555555555555555555555555555555555555555555555555556, 3, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 2, 3, 300 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract ProductCheckSize {
    // The factor `y` never changes in the loop, so under `-Osize` the product's check compares
    // `x` with `MAX / y - (y == 0)`, computed once before the loop: the loop body keeps its `mul`
    // but no `div`. A zero factor never overflows, and the compare joins the addition's overflow
    // check in one branch.
    // CHECK-LABEL: @module ProductCheckSize_runtime
    // CHECK: div
    // CHECK: [loop]:
    // CHECK-NOT: div
    // CHECK: gt
    function accumulate(uint256 x, uint256 y, uint256 n) external pure returns (uint256) {
        for (uint256 i = 0; i < n; ++i) {
            x = x * y + 1;
        }
        return x;
    }
}
