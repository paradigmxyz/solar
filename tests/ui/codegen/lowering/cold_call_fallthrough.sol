//@ revisions: none size
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE --enable-var-scope
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime --pretty-json
//@[size] filecheck: --check-prefix=SIZE --enable-var-scope

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough after
// optimization, while the unoptimized revision retains the explicit jumps.
contract ColdCallFallthrough {
    // NONE-LABEL: @module ColdCallFallthrough_runtime
    // NONE: jump {{bb[0-9]+}}
    // NONE-NEXT: [[WRAPPER:bb[0-9]+]]:
    // NONE: jumpi [[SHORT:bb[0-9]+]], [[VALID:bb[0-9]+]]
    // NONE-NEXT: [[BODY:bb[0-9]+]]:
    // NONE-NEXT: push 0
    // NONE-NEXT: push 4
    // NONE-NEXT: calldataload
    // NONE-NEXT: eq
    // NONE-NEXT: jumpi [[NONE_COLD:bb[0-9]+]], [[HOT_EDGE:bb[0-9]+]]
    // NONE-NEXT: [[NONE_COLD]]:
    // NONE-NEXT: push 4
    // NONE-NEXT: calldataload
    // NONE-NEXT: push 224
    // NONE-NEXT: mstore
    // NONE-NEXT: jump [[ABORT:bb[0-9]+]]
    // NONE-NEXT: [[HOT_EDGE]]:
    // NONE-NEXT: jump [[HOT:bb[0-9]+]]
    // NONE-NEXT: [[HOT]]:
    // NONE: return
    // NONE: [[VALID]]:
    // NONE-NEXT: jump [[BODY]]
    // NONE-NEXT: [[ENTRY:bb[0-9]+]]:
    // NONE-NEXT: jump [[WRAPPER]]
    // NONE: [[ABORT]]:
    // NONE-NEXT: push 224
    // NONE-NEXT: mload
    // NONE-NEXT: jump {{bb[0-9]+}}
    // NONE: push 0x8c379a000000000000000000000000000000000000000000000000000000000
    // NONE: revert
    // NONE: callvalue
    // NONE: shr
    // NONE-NEXT: jump [[DISPATCH:bb[0-9]+]]
    // NONE-NEXT: [[CASE:bb[0-9]+]]:
    // NONE-NEXT: jump [[ENTRY]]
    // NONE: push 0x4b692dff
    // NONE-NEXT: sub
    // NONE-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
    // NONE-NEXT: [[TAKEN:bb[0-9]+]]:
    // NONE-NEXT: pop
    // NONE-NEXT: jump [[CASE]]
    // NONE-NEXT: [[DISPATCH]]:
    // NONE-NEXT: dup 1
    // NONE-NEXT: push 0x161e4029
    // NONE-NEXT: eq
    // NONE-NEXT: jumpi [[TAKEN]], {{bb[0-9]+}}

    // SIZE-LABEL: @module ColdCallFallthrough_runtime
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
