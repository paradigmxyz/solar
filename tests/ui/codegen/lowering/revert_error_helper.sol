//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// Constant short revert messages use a shared helper when the value comes from
// a local or library constant. Literal messages write their ABI payload in the
// cold path, while messages longer than one word use the generic encoder.

library Errors {
    string public constant SHORT = "39";
    string public constant LONG = "this-is-a-33-byte-long-message!!!";
}

contract R {
    string constant LOCAL = "local-const-msg";

    // CHECK-LABEL: revert_error_helper.sol:R (runtime)
    // CHECK-NEXT: @module runtime
    // CHECK: indexed_jump
    // CHECK: push 0x3339
    // CHECK: push 240
    // CHECK: jump [[SHORT_HELPER:bb[0-9]+]]
    // CHECK: [[SHORT_HELPER]] [cold]:
    // CHECK: shl
    // CHECK: [[LONG_HELPER:bb[0-9]+]] [cold]:
    // CHECK: push 0x8c379a0
    // CHECK: mcopy
    // CHECK: revert
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

    // The block layout puts the constant call sites before the literal helper.
    // CHECK: push 0x6c6f63616c2d636f6e73742d6d7367
    // CHECK: jump [[SHORT_HELPER]]
    // CHECK: push 0x746869732d69732d612d33332d627974652d6c6f6e672d6d6573736167652121
    // CHECK: jump [[LONG_HELPER]]
    // CHECK: push 0x6c69746572616c206d7367
    // CHECK: jump [[LITERAL_HELPER:bb[0-9]+]]
    // CHECK: [[LITERAL_HELPER]] [cold]:
    // CHECK: shl
    // CHECK: push 68
    // CHECK: mstore
    // CHECK: push 100
    // CHECK: push 0
    // CHECK: revert
    // CHECK: push 0x7265766572742d70617468
    // CHECK: jump [[LITERAL_HELPER]]
    function viaRevertMsg(uint256 x) external pure returns (uint256) {
        if (x <= 5) {
            revert("revert-path");
        }
        return x;
    }
}
