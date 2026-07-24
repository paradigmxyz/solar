//@compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough.
contract ColdCallHotFallthrough {
    // CHECK: push 0x161e4029
    // CHECK: eq
    // CHECK-NEXT: push [[NONZERO:bb[0-9]+]]
    // CHECK: push 0x4b692dff
    // CHECK: eq
    // CHECK-NEXT: push [[BELOW:bb[0-9]+]]
    // CHECK: [[NONZERO]]:
    // CHECK: iszero
    // CHECK-NEXT: push [[ABORT:bb[0-9]+]]
    // CHECK-NEXT: jump [[BRANCH:bb[0-9]+]]
    // CHECK: [[BRANCH]]:
    // CHECK-NEXT: jumpi
    // CHECK: return
    function nonzero(uint256 value) external pure returns (uint256) {
        if (value == 0) abort(value);
        return value;
    }

    // CHECK: [[BELOW]]:
    // CHECK: push 100
    // CHECK: lt
    // CHECK-NEXT: push [[ABORT]]
    // CHECK-NEXT: jump [[BRANCH]]
    // CHECK: push 0x6f6e65
    // CHECK: jump [[REVERT_ERROR:bb[0-9]+]]
    // CHECK: [[REVERT_ERROR]] [cold]:
    // CHECK-NEXT: shl
    // CHECK: push 0x8c379a0
    // CHECK: revert
    function belowLimit(uint256 value) external pure returns (uint256) {
        if (value < 100) abort(value);
        return value;
    }

    // Keep this large enough to remain a call after the inlining pass.
    // CHECK: [[ABORT]] [cold]:
    // CHECK: push 0x7a65726f
    // CHECK: jump [[REVERT_ERROR]]
    function abort(uint256 value) internal pure {
        if (value == 0) revert("zero");
        if (value == 1) revert("one");
        if (value == 2) revert("two");
        revert("other");
    }
}
