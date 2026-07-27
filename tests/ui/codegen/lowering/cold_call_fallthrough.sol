//@ revisions: none size
//@[none] compile-flags: -Zcodegen -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE --enable-var-scope
//@[size] compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime --pretty-json
//@[size] filecheck: --check-prefix=SIZE --enable-var-scope

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough after
// optimization, while the unoptimized revision retains the explicit jumps.
contract ColdCallFallthrough {
    // NONE-LABEL: @module runtime
    // NONE: eq
    // NONE-NEXT: push [[NONE_DISPATCH:bb[0-9]+]]
    // NONE-NEXT: jumpi
    // NONE: [[NONE_DISPATCH]]:
    // NONE-NEXT: jump [[WRAPPER:bb[0-9]+]]
    // NONE: [[WRAPPER]]:
    // NONE: eq
    // NONE-NEXT: iszero
    // NONE-NEXT: push [[HOT:bb[0-9]+]]
    // NONE-NEXT: jumpi
    // NONE-NEXT: jump [[NONE_COLD:bb[0-9]+]]
    // NONE: [[NONE_COLD]]:
    // NONE: jump
    // NONE: [[HOT]]:
    // NONE: return

    // SIZE-LABEL: @module runtime
    // SIZE: eq
    // SIZE-NEXT: push [[SIZE_DISPATCH:bb[0-9]+]]
    // SIZE-NEXT: jumpi
    // SIZE: [[SIZE_DISPATCH]]:
    // SIZE: iszero
    // SIZE-NEXT: push [[SIZE_COLD:bb[0-9]+]]
    // SIZE-NEXT: jump [[BRANCH:bb[0-9]+]]
    // SIZE: [[BRANCH]]:
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
