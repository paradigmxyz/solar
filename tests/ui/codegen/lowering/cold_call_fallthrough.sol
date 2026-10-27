//@ revisions: none size
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE --enable-var-scope
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime --pretty-json
//@[size] filecheck: --check-prefix=SIZE --enable-var-scope

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough after
// optimization, while the unoptimized revision does not mark the call blocks cold.
contract ColdCallFallthrough {
    // NONE-LABEL: @module ColdCallFallthrough_runtime
    // NONE: eq
    // NONE-NEXT: push [[WRAPPER:bb[0-9]+]]
    // NONE-NEXT: jumpi
    // NONE: [[WRAPPER]]:
    // NONE: iszero
    // NONE-NEXT: push [[NONE_CALL:bb[0-9]+]]
    // NONE-NEXT: jumpi
    // NONE: return
    // NONE: [[NONE_CALL]]:
    // NONE-NOT: [cold]

    // SIZE-LABEL: @module ColdCallFallthrough_runtime
    // SIZE: eq
    // SIZE-NEXT: push [[SIZE_DISPATCH:bb[0-9]+]]
    // SIZE-NEXT: jumpi
    // SIZE: [[SIZE_DISPATCH]]:
    // SIZE: iszero
    // SIZE-NEXT: jump [[BRANCH:bb[0-9]+]]
    // SIZE: [[BRANCH]]:
    // SIZE-NEXT: push [[SIZE_COLD:bb[0-9]+]]
    // SIZE-NEXT: jumpi
    // SIZE-NOT: jump
    // SIZE: return
    // SIZE: [[SIZE_COLD]] [cold]:
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
