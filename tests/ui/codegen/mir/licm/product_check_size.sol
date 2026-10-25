//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: accumulate 3, 0, 4 => 0
//@ run-call: accumulate 3, 7, 0 => 3
//@ run-call: accumulate 2, 3, 5 => 849
//@ run-call: accumulate 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: accumulate 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 2, 1 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call-fail: accumulate 0x8000000000000000000000000000000000000000000000000000000000000000, 2, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0x5555555555555555555555555555555555555555555555555555555555555556, 3, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 2, 3, 300 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract ProductCheckSize {
    // The factor `y` never changes in the loop, so under `-Osize` the product's check compares
    // `x` with `MAX / y - (y == 0)`, computed once before the loop: the loop body keeps its `mul`
    // but no `div`. A zero factor never overflows, and the compare is the branch condition
    // itself, so no `iszero` follows it and it joins the addition's overflow check in one `or`
    // and one branch.
    // CHECK-LABEL: @module ProductCheckSize_runtime
    // CHECK: not
    // CHECK-NEXT: div
    // CHECK-NEXT: sub
    // CHECK: [loop]:
    // CHECK: [loop]:
    // CHECK-NOT: {{div|iszero|jumpi}}
    // CHECK: or
    // CHECK-NEXT: push bb{{[0-9]+}}
    // CHECK-NEXT: jumpi
    function accumulate(uint256 x, uint256 y, uint256 n) external pure returns (uint256) {
        for (uint256 i = 0; i < n; ++i) {
            x = x * y + y;
        }
        return x;
    }
}
