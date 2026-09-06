//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// The Yul `return(offset, size)` builtin halts execution and returns `size`
// bytes of memory (the `RETURN` opcode), like the `ret_data` terminator. It is
// used by delegatecall proxy fallbacks (e.g. OpenZeppelin `Proxy`). Runtime
// behavior is verified against solc 0.8.30 separately.

contract R {
    // CHECK-LABEL: @module R_runtime
    // CHECK: push 0x6279e43c
    // CHECK: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL:bb[0-9]+]], [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[FAIL]]{{( \[cold\])?}}:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    function echo(uint256 x) external pure returns (uint256) {
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }
}
