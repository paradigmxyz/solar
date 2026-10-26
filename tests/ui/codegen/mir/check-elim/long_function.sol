//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: chained 1, 2, 100 => 197
//@ run-call: chained 0, 5, 200 => 395
//@ run-call-fail: chained 1, 2, 3 => 0xaf7cfeb1
//@ run-call-fail: chained 2, 1, 100 => 0x
//@ run-call: unchained 1, 2, 100 => 197
//@ run-call-fail: unchained 1, 2, 1 => 0xaf7cfeb1
//@ run-call-fail: unchained 1, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// The guards keep each function above the 128 blocks from which the dominator walk lists the
// relations its scope states. The searches through them still prove `a < c` from `a < b` and
// `b < c`, so neither difference checks its underflow, however many branches precede them.
// CHECK-LABEL: fn @chained()
// CHECK-NOT: lt arg2, arg1
// CHECK-NOT: lt arg2, arg0
//
// Without the second guard, `b` is unordered against `c` and its difference keeps the check.
// CHECK-LABEL: fn @unchained()
// CHECK: lt arg2, arg1

error Hole();

contract LongFunction {
    function chained(uint256 a, uint256 b, uint256 c) external pure returns (uint256 r) {
        require(a < b);
        require(b < c);
        if (c == 1) revert Hole();
        if (c == 2) revert Hole();
        if (c == 3) revert Hole();
        if (c == 4) revert Hole();
        if (c == 5) revert Hole();
        if (c == 6) revert Hole();
        if (c == 7) revert Hole();
        if (c == 8) revert Hole();
        if (c == 9) revert Hole();
        if (c == 10) revert Hole();
        if (c == 11) revert Hole();
        if (c == 12) revert Hole();
        if (c == 13) revert Hole();
        if (c == 14) revert Hole();
        if (c == 15) revert Hole();
        if (c == 16) revert Hole();
        if (c == 17) revert Hole();
        if (c == 18) revert Hole();
        if (c == 19) revert Hole();
        if (c == 20) revert Hole();
        if (c == 21) revert Hole();
        if (c == 22) revert Hole();
        if (c == 23) revert Hole();
        if (c == 24) revert Hole();
        if (c == 25) revert Hole();
        if (c == 26) revert Hole();
        if (c == 27) revert Hole();
        if (c == 28) revert Hole();
        if (c == 29) revert Hole();
        if (c == 30) revert Hole();
        if (c == 31) revert Hole();
        if (c == 32) revert Hole();
        if (c == 33) revert Hole();
        if (c == 34) revert Hole();
        if (c == 35) revert Hole();
        if (c == 36) revert Hole();
        if (c == 37) revert Hole();
        if (c == 38) revert Hole();
        if (c == 39) revert Hole();
        if (c == 40) revert Hole();
        if (c == 41) revert Hole();
        if (c == 42) revert Hole();
        if (c == 43) revert Hole();
        if (c == 44) revert Hole();
        if (c == 45) revert Hole();
        if (c == 46) revert Hole();
        if (c == 47) revert Hole();
        if (c == 48) revert Hole();
        if (c == 49) revert Hole();
        if (c == 50) revert Hole();
        r = (c - b) + (c - a);
    }

    function unchained(uint256 a, uint256 b, uint256 c) external pure returns (uint256 r) {
        require(a < b);
        if (c == 1) revert Hole();
        if (c == 2) revert Hole();
        if (c == 3) revert Hole();
        if (c == 4) revert Hole();
        if (c == 5) revert Hole();
        if (c == 6) revert Hole();
        if (c == 7) revert Hole();
        if (c == 8) revert Hole();
        if (c == 9) revert Hole();
        if (c == 10) revert Hole();
        if (c == 11) revert Hole();
        if (c == 12) revert Hole();
        if (c == 13) revert Hole();
        if (c == 14) revert Hole();
        if (c == 15) revert Hole();
        if (c == 16) revert Hole();
        if (c == 17) revert Hole();
        if (c == 18) revert Hole();
        if (c == 19) revert Hole();
        if (c == 20) revert Hole();
        if (c == 21) revert Hole();
        if (c == 22) revert Hole();
        if (c == 23) revert Hole();
        if (c == 24) revert Hole();
        if (c == 25) revert Hole();
        if (c == 26) revert Hole();
        if (c == 27) revert Hole();
        if (c == 28) revert Hole();
        if (c == 29) revert Hole();
        if (c == 30) revert Hole();
        if (c == 31) revert Hole();
        if (c == 32) revert Hole();
        if (c == 33) revert Hole();
        if (c == 34) revert Hole();
        if (c == 35) revert Hole();
        if (c == 36) revert Hole();
        if (c == 37) revert Hole();
        if (c == 38) revert Hole();
        if (c == 39) revert Hole();
        if (c == 40) revert Hole();
        if (c == 41) revert Hole();
        if (c == 42) revert Hole();
        if (c == 43) revert Hole();
        if (c == 44) revert Hole();
        if (c == 45) revert Hole();
        if (c == 46) revert Hole();
        if (c == 47) revert Hole();
        if (c == 48) revert Hole();
        if (c == 49) revert Hole();
        if (c == 50) revert Hole();
        r = (c - b) + (c - a);
    }
}
