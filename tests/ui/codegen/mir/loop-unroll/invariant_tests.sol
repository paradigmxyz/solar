//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: periodStarts 1700000000, 86400, 0 => 0
//@ run-call: periodStarts 1700000000, 86400, 1 => 1699920000
//@ run-call: periodStarts 1700000000, 86400, 3 => 5099846400
//@ run-call: periodStarts 1700000000, 86400, 64 => 108804729600
//@ run-call: periodStarts 5, 0, 0 => 0
//@ run-call-fail: periodStarts 5, 0, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: periodStarts 5, 0, 7 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: keep 1000000000000000000, 3, 0 => 0
//@ run-call: keep 1000000000000000000, 3, 1 => 666666666666666667
//@ run-call: keep 1000000000000000000, 3, 5 => 10000000000000000002
//@ run-call: keep 7, 0, 0 => 0
//@ run-call-fail: keep 7, 0, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: keep 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Dividing by a word the loop never changes tests it for zero in every
// iteration. Gas builds run the first iteration in a copy that keeps the test;
// the loop after it skips the test and unrolls by two, and a loop that runs no
// iteration never tests the divisor.
contract InvariantTests {
    // CHECK-LABEL: fn @periodStarts()
    // CHECK: [[NONZERO:v[0-9]+]] = ne arg1, 0
    // CHECK: jumpi [[NONZERO]], [[FIRST:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: [[FIRST]]:
    // CHECK-NEXT: div arg0, arg1
    // CHECK-NOT: jumpi [[NONZERO]]
    // CHECK: [[BIT:v[0-9]+]] = and {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ODD:v[0-9]+]] = ne [[BIT]], 0
    // CHECK-NEXT: jumpi [[ODD]],
    // CHECK-NOT: jumpi [[NONZERO]]
    // CHECK-COUNT-3: div {{v[0-9]+}}, arg1
    // CHECK-LABEL: fn @keep()
    function periodStarts(uint256 time, uint256 period, uint256 count)
        external
        pure
        returns (uint256 total)
    {
        for (uint256 i = 0; i < count; ++i) {
            total += time / period * period;
            time += 1 hours;
        }
    }

    function keep(uint256 amount, uint256 shares, uint256 count)
        external
        pure
        returns (uint256 total)
    {
        for (uint256 i = 0; i < count; ++i) {
            total += amount - amount / shares;
            amount += 1 ether;
        }
    }
}
