//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas --optimize-runs 10000 -Zdump=mir
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
//@ run-call-fail: accumulate 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff6, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeb, 8 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdb, 9 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffdc, 9 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: toWad 1, 1, 3 => 0x574fbde6000
//@ run-call: toWad 0x119799812dea11197f27f0f6e885c8ba7eb31f476caf7411a863386, 1, 1 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffe4290966000
//@ run-call-fail: toWad 0x119799812dea11197f27f0f6e885c8ba7eb31f476caf7411a863386, 1, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: toWad 0x119799812dea11197f27f0f6e885c8ba7eb31f476caf7411a863387, 0, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffff2b653b7000
//@ run-call-fail: toWad 0x119799812dea11197f27f0f6e885c8ba7eb31f476caf7411a863388, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: toWad 0x119799812dea11197f27f0f6e885c8ba7eb31f476caf7411a863388, 1, 0 => 0
//@ run-call-fail: toWad 5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: toWad 0, 0x232f33025bd42232fe4fe1edd10b9174fd663e8ed95ee82350c670, 4 => 0xbffffffffffffffffffffffffffffffffffffffffffffffffffffa9a2f8a0000
//@ run-call-fail: toWad 0, 0x232f33025bd42232fe4fe1edd10b9174fd663e8ed95ee82350c670, 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Overflow checks that branch to the same panic merge into one test; whichever
// addition overflows, the call still reverts with `Panic(0x11)`, from either
// check of the straight-line sum and from any copy of the unrolled loop.
// CHECK-LABEL: fn @entry()
// `sum3` tests both additions with one branch.
// CHECK: [[FIRST:v[0-9]+]] = lt {{v[0-9]+}}, {{v[0-9]+}}
// CHECK: [[SECOND:v[0-9]+]] = lt {{v[0-9]+}}, {{v[0-9]+}}
// CHECK-NEXT: [[EITHER:v[0-9]+]] = or [[FIRST]], [[SECOND]]
// CHECK-NEXT: jumpi [[EITHER]], [[PANIC:bb[0-9]+]], {{bb[0-9]+}}
// The unrolled loop adds four counters that cannot reach `2^256` together, so
// one test that the last sum fell below the first replaces the four tests.
// CHECK: {{v[0-9]+}} = phi [{{bb[0-9]+}}: 0], {{\[}}{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK-NEXT: [[START:v[0-9]+]] = phi
// CHECK: [[WRAPPED:v[0-9]+]] = lt {{v[0-9]+}}, [[START]]
// CHECK-NEXT: jumpi [[WRAPPED]], [[PANIC]], {{bb[0-9]+}}
contract MergeAborts {
    function sum3(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b + c;
    }

    function accumulate(uint256 start, uint256 n) external pure returns (uint256 acc) {
        acc = start;
        for (uint256 i; i < n; ++i) acc += i;
    }

    // In a loop, the product's range check aborts on its false edge and flips to
    // merge with the additions' checks.
    function toWad(uint256 amount, uint256 step, uint256 count)
        external
        pure
        returns (uint256 total)
    {
        for (uint256 i = 0; i < count; ++i) {
            total += amount * 1e12;
            amount += step;
        }
    }
}
