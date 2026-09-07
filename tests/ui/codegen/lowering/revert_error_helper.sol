//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// Constant short revert messages share one synthesized `__revert_error`
// helper per module: each `require`/`revert` site passes the length and the
// left-aligned data word instead of materializing and ABI-encoding the string
// in place (~60-90 bytes per site — aave's `Errors.X` constants alone account
// for kilobytes). The constant may be a literal, a local `constant`, or a
// library `constant` reached through a member access. Messages longer than
// one word materialize their resolved bytes and use the generic encoder —
// resolving through `lower_expr` would truncate the constant to one word.
// Revert data is byte-identical to solc 0.8.30 for every shape (verified on
// anvil, including the 33-byte and empty-string edges).

library Errors {
    string public constant SHORT = "39";
    string public constant LONG = "this-is-a-33-byte-long-message!!!";
}

contract R {
    string constant LOCAL = "local-const-msg";

    // These checks retain one shared fixed Error encoder and its four callers.
    // Short-message shifts are site-local: the former shared WORD11 preparation
    // is intentionally not required when direct stack entry removes frame traffic.
    // CHECK-LABEL: @module R_runtime
    // CHECK: callvalue
    // CHECK-NEXT: jumpi [[FAIL:bb[0-9]+]], [[DISPATCH:bb[0-9]+]]
    // CHECK: [[DISPATCH]]:
    // CHECK: indexed_jump
    // CHECK: [[SUCCESS:bb[0-9]+]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[FAIL]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: jump [[LONG_COPY:bb[0-9]+]]
    // CHECK: [[LONG_COPY]] [cold]:
    // CHECK: mcopy
    // CHECK: revert
    // CHECK: push 0x9ee36b07
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[LIB_DECODE:bb[0-9]+]]
    // CHECK: [[LIB_DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[LIB_CHECK:bb[0-9]+]]
    // CHECK: [[LIB_CHECK]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[SUCCESS]], [[LIB_ERROR:bb[0-9]+]]
    // CHECK: [[LIB_ERROR]] [cold]:
    // CHECK-NEXT: push 2
    // CHECK-NEXT: push 0x3339
    // CHECK-NEXT: push 240
    // CHECK-NEXT: shl
    // CHECK-NEXT: jump [[ERROR:bb[0-9]+]]
    // CHECK: [[ERROR]] [cold]:
    // CHECK-NEXT: push 0x461bcd
    // CHECK-NEXT: push 229
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 4
    // CHECK-NEXT: mstore
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push 36
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 68
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 100
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: push 0xc4186a6
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[LONG_DECODE:bb[0-9]+]]
    // CHECK: [[LONG_DECODE]]:
    // CHECK: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[LONG_CHECK:bb[0-9]+]]
    // CHECK: [[LONG_CHECK]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK: push 33
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mstore
    // CHECK: push 0x746869732d69732d612d33332d627974652d6c6f6e672d6d6573736167652121
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mstore
    // CHECK: push 33
    // CHECK-NEXT: push 248
    // CHECK-NEXT: shl
    // CHECK: jumpi [[SUCCESS]], [[LONG_ERROR:bb[0-9]+]]
    // CHECK: [[LONG_ERROR]] [cold]:
    // CHECK: push 0x461bcd
    // CHECK-NEXT: push 229
    // CHECK-NEXT: shl
    // CHECK: jumpi {{bb[0-9]+}}, [[LONG_COPY]]
    // CHECK: push 0x9af992c0
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[LITERAL_DECODE:bb[0-9]+]]
    // CHECK: [[LITERAL_DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[LITERAL_CHECK:bb[0-9]+]]
    // CHECK: [[LITERAL_CHECK]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[SUCCESS]], [[LITERAL_ERROR:bb[0-9]+]]
    // CHECK: [[LITERAL_ERROR]] [cold]:
    // CHECK-NEXT: push 11
    // CHECK-NEXT: push 0x6c69746572616c206d7367
    // CHECK-NEXT: push 168
    // CHECK-NEXT: shl
    // CHECK-NEXT: jump [[ERROR]]
    // CHECK: push 0x17a0525e
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[REVERT_DECODE:bb[0-9]+]]
    // CHECK: [[REVERT_DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[REVERT_CHECK:bb[0-9]+]]
    // CHECK: [[REVERT_CHECK]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[SUCCESS]], [[REVERT_ERROR:bb[0-9]+]]
    // CHECK: [[REVERT_ERROR]] [cold]:
    // CHECK-NEXT: push 11
    // CHECK-NEXT: push 0xe4caeccae4e85ae0c2e8d
    // CHECK-NEXT: push 171
    // CHECK-NEXT: shl
    // CHECK-NEXT: jump [[ERROR]]
    // CHECK: push 0x9927bee4
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[LOCAL_DECODE:bb[0-9]+]]
    // CHECK: [[LOCAL_DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[LOCAL_CHECK:bb[0-9]+]]
    // CHECK: [[LOCAL_CHECK]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[SUCCESS]], [[LOCAL_ERROR:bb[0-9]+]]
    // CHECK: [[LOCAL_ERROR]] [cold]:
    // CHECK-NEXT: push 15
    // CHECK-NEXT: push 0x6c6f63616c2d636f6e73742d6d7367
    // CHECK-NEXT: push 136
    // CHECK-NEXT: shl
    // CHECK-NEXT: jump [[ERROR]]

    function viaLibConst(uint256 x) external pure returns (uint256) {
        require(x > 5, Errors.SHORT);
        return x;
    }

    function viaLiteral(uint256 x) external pure returns (uint256) {
        require(x > 5, "literal msg");
        return x;
    }

    function viaLocalConst(uint256 x) external pure returns (uint256) {
        require(x > 5, LOCAL);
        return x;
    }

    function viaLong(uint256 x) external pure returns (uint256) {
        require(x > 5, Errors.LONG);
        return x;
    }

    function viaRevertMsg(uint256 x) external pure returns (uint256) {
        if (x <= 5) {
            revert("revert-path");
        }
        return x;
    }
}
