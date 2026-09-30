//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@ run-call: first 7 => 22
//@ run-call: first 31 => 94
//@ run-call: second 7 => 6
//@ run-call: sum 0 => 0
//@ run-call: sum 64 => 5088
//@ run-call: sum 1000 => 78444
//@ run-call-fail: first 32 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: second 64 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001

// Inline the checked bodies but keep the outlined panic payload.
// CHECK: @module GuardedCalls
// CHECK-NOT: icall @left
// CHECK-NOT: icall @right
contract GuardedCalls {
    function left(uint256 x) private pure returns (uint256) {
        assert(x < 32);
        unchecked { return x + 1; }
    }

    function right(uint256 x) private pure returns (uint256) {
        assert(x < 64);
        unchecked { return x * 2; }
    }

    function first(uint256 x) external pure returns (uint256) {
        unchecked { return left(x) + right(x); }
    }

    function second(uint256 x) external pure returns (uint256) {
        return right(x) ^ left(x);
    }

    // Masked arguments make the inlined panic checks redundant in the loop.
    // CHECK-LABEL: fn @sum(
    // CHECK-NOT: icall
    // CHECK-NOT: tail_call
    // CHECK: fn @
    function sum(uint256 count) external pure returns (uint256 total) {
        unchecked {
            for (uint256 i; i < count; ++i) {
                total += left(i & 31) + right(i & 63);
            }
        }
    }
}
