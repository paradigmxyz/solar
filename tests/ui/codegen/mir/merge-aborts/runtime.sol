//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: sum3 1, 2, 3 => 6
//@ run-call-fail: sum3 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: sum3 1, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: sum3 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: accumulate 5, 8 => 33
//@ run-call: accumulate 0, 0 => 0
//@ run-call: accumulate 7, 1 => 7
//@ run-call: accumulate 0, 100 => 4950
//@ run-call: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe3, 8 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe4, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Overflow checks that branch to the same panic merge into one test; whichever
// addition overflows, the call still reverts with `Panic(0x11)`, from either
// check of the straight-line sum and from any copy of the unrolled loop.
// CHECK-LABEL: fn @entry()
// `sum3` tests both additions with one branch.
// CHECK: [[FIRST:v[0-9]+]] = lt {{v[0-9]+}}, {{v[0-9]+}}
// CHECK: [[SECOND:v[0-9]+]] = lt {{v[0-9]+}}, {{v[0-9]+}}
// CHECK-NEXT: [[EITHER:v[0-9]+]] = or [[FIRST]], [[SECOND]]
// CHECK-NEXT: jumpi [[EITHER]], [[PANIC:bb[0-9]+]], {{bb[0-9]+}}
// The unrolled loop tests the four additions of its copies with one branch.
// CHECK-COUNT-3: = or
// CHECK-NEXT: jumpi {{v[0-9]+}}, [[PANIC]], {{bb[0-9]+}}
contract MergeAborts {
    function sum3(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b + c;
    }

    function accumulate(uint256 start, uint256 n) external pure returns (uint256 acc) {
        acc = start;
        for (uint256 i; i < n; ++i) acc += i;
    }
}
