//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// Constant short revert messages use one shared helper per module. Literal and
// constant messages pass their length and left-aligned data word to the helper;
// messages longer than one word use the generic encoder.

library Errors {
    string public constant SHORT = "39";
    string public constant LONG = "this-is-a-33-byte-long-message!!!";
}

contract R {
    string constant LOCAL = "local-const-msg";

    // CHECK-LABEL: revert_error_helper.sol:R (runtime)
    // CHECK: @module runtime
    // CHECK: indexed_jump
    // CHECK: push 0x3339
    // CHECK: push 240
    // CHECK: shl
    // CHECK: push 2
    // CHECK: swap 1
    // CHECK: jump [[ERROR_HELPER:bb[0-9]+]]
    // CHECK: [[ERROR_HELPER]] [cold]:
    // CHECK: push 0x8c379a0
    // CHECK: push 224
    // CHECK: shl
    // CHECK: push 0
    // CHECK: mstore
    // CHECK: push 32
    // CHECK: push 4
    // CHECK: mstore
    // CHECK: push 100
    // CHECK: push 0
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
    // CHECK: push 0x6c69746572616c206d7367
    // CHECK: jump [[LITERAL_HELPER:bb[0-9]+]]
    // CHECK: [[LITERAL_HELPER]] [cold]:
    // CHECK: push 168
    // CHECK: shl
    // CHECK: push 11
    // CHECK: swap 1
    // CHECK: jump [[ERROR_HELPER]]
    function viaRevertMsg(uint256 x) external pure returns (uint256) {
        if (x <= 5) {
            revert("revert-path");
        }
        return x;
    }
}
