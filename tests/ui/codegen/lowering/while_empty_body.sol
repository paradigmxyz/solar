//@compile-flags: -Zdump=evm-ir
//@ filecheck:

contract WhileEmptyBody {
    // CHECK-LABEL: @module WhileEmptyBody_runtime
    // CHECK: push 0xb3de648b
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL:bb[0-9]+]], [[HEAD:bb[0-9]+]]
    // CHECK: [[HEAD]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: jumpi [[LOOP]], [[EXIT:bb[0-9]+]]
    // CHECK: [[EXIT]]:
    // CHECK-NEXT: stop
    // CHECK: [[FAIL]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    function f(uint256 x) public pure {
        while (x > 0) {}
    }
}
