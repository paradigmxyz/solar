//@compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime --pretty-json
//@filecheck: --enable-var-scope

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough.
contract ColdCallHotFallthrough {
    // CHECK-LABEL: @module runtime
    // CHECK: sgt
    // CHECK-NEXT: push [[REVERT:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 0{{$}}
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 4{{$}}
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push [[COLD:bb[0-9]+]]
    // CHECK-NEXT: jump [[BRANCH:bb[0-9]+]]
    // CHECK: [[BRANCH]]:
    // CHECK-NEXT: jumpi
    // CHECK: return
    // CHECK: [[COLD]] [cold]:
    function nonzero(uint256 value) external pure returns (uint256) {
        if (value == 0) abort(value);
        return value;
    }

    function belowLimit(uint256 value) external pure returns (uint256) {
        if (value < 100) abort(value);
        return value;
    }

    // Keep this large enough to remain a call after the inlining pass.
    function abort(uint256 value) internal pure {
        if (value == 0) revert("zero");
        if (value == 1) revert("one");
        if (value == 2) revert("two");
        revert("other");
    }
}
