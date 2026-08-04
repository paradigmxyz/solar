//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
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

    // CHECK-LABEL: @module runtime
    // CHECK: push 0xc4186a6
    // CHECK: push 0x17a0525e
    // CHECK: push 0x9927bee4
    // CHECK: push 0x9af992c0
    // CHECK: push 0x9ee36b07
    // CHECK: push 0x3339
    // CHECK: push 0x8c379a0
    // CHECK: revert
    // CHECK: push 0x6c69746572616c206d7367
    // CHECK: push 0x6c6f63616c2d636f6e73742d6d7367
    // CHECK: push 0x746869732d69732d612d33332d627974652d6c6f6e672d6d6573736167652121
    // CHECK: push 0x7265766572742d70617468
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
