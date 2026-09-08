//@ revisions: none size mir
//@[mir] compile-flags: -O none -Zdump=mir
//@[none,size,mir] run-call-fail: nonzero 0 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000047a65726f00000000000000000000000000000000000000000000000000000000
//@[none,size,mir] run-call: nonzero 1 => 1
//@[none,size,mir] run-call-fail: belowLimit 1 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000036f6e650000000000000000000000000000000000000000000000000000000000
//@[none,size,mir] run-call-fail: belowLimit 2 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000374776f0000000000000000000000000000000000000000000000000000000000
//@[none,size,mir] run-call-fail: belowLimit 3 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000056f74686572000000000000000000000000000000000000000000000000000000
//@[none,size,mir] run-call: belowLimit 100 => 100
//@[none,size,mir] run-call-fail: 0x161e4029 => 0x
//@[none,size,mir] run-call-fail: 0x => 0x
//@[none,size,mir] run-call-fail: 0xdeadbeef => 0x
//@[none,size,mir] run-call-fail: nonzero 1; value=1 => 0x
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefix=NONE --enable-var-scope
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime --pretty-json
//@[size] filecheck: --check-prefix=SIZE --enable-var-scope

// Non-returning helper paths stay cold and preserve each validated argument.
// Cost-based layout may reach the shared successful terminal by a taken edge.
// Matched sealed gas and size checks supersede the former two-fallthrough
// strategy. None retains its explicit jumps.
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
    // SIZE: callvalue
    // SIZE-NEXT: jumpi [[REJECT:bb[0-9]+]], [[DISPATCH:bb[0-9]+]]
    // SIZE: [[DISPATCH]]:
    // SIZE: push 0x161e4029
    // SIZE-NEXT: eq
    // SIZE-NEXT: jumpi [[NONZERO:bb[0-9]+]], [[OTHER:bb[0-9]+]]
    // SIZE: [[OTHER]]:
    // SIZE-NEXT: push 0x4b692dff
    // SIZE-NEXT: sub
    // SIZE-NEXT: jumpi [[REJECT]], [[BELOW:bb[0-9]+]]
    // SIZE: [[BELOW]]:
    // SIZE-NEXT: calldatasize
    // SIZE-NEXT: push 36
    // SIZE-NEXT: gt
    // SIZE-NEXT: jumpi [[REJECT]], [[LIMIT:bb[0-9]+]]
    // SIZE: [[LIMIT]]:
    // SIZE-NEXT: push 100
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: lt
    // SIZE-NEXT: jumpi [[BELOW_COLD:bb[0-9]+]], [[RETURN:bb[0-9]+]]
    // SIZE: [[RETURN]]:
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: push 0
    // SIZE-NEXT: mstore
    // SIZE-NEXT: push 32
    // SIZE-NEXT: push 0
    // SIZE-NEXT: return
    // SIZE: [[REJECT]] [cold]:
    // SIZE-NEXT: push 0
    // SIZE-NEXT: push 0
    // SIZE-NEXT: revert
    // SIZE: [[BELOW_COLD]] [cold]:
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: jump [[ABORT:bb[0-9]+]]
    // SIZE: [[NONZERO]]:
    // SIZE-NEXT: calldatasize
    // SIZE-NEXT: push 36
    // SIZE-NEXT: gt
    // SIZE-NEXT: jumpi [[REJECT]], [[VALUE:bb[0-9]+]]
    // SIZE: [[VALUE]]:
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: jumpi [[RETURN]], [[ZERO_COLD:bb[0-9]+]]
    // SIZE: [[ZERO_COLD]] [cold]:
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: jump [[ABORT]]
    // SIZE: [[ABORT]] [cold]:
    // SIZE-NEXT: dup 1
    // SIZE-NEXT: jumpi [[OTHER_ERROR:bb[0-9]+]], [[ZERO_ERROR:bb[0-9]+]]
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
