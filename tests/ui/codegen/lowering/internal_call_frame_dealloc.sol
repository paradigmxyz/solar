//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck: --implicit-check-not=mload

contract ICallFrameDealloc {
    // Scalar recursion passes its arguments and result on the stack, so this
    // case needs no memory-frame allocation or deallocation between calls.
    // CHECK-LABEL: @module InternalCallFrameDealloc_runtime
    // CHECK: push 0xb3de648b
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi
    // CHECK: push [[FIRST_RET:bb[0-9]+]]
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: jump [[SUM_ENTRY:bb[0-9]+]]
    // CHECK: [[SECOND_RET:bb[0-9]+]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK: return
    // CHECK: [[FIRST_RET]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK: push [[SECOND_RET]]
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump [[SUM_ENTRY]]
    // CHECK: [[SUM_ENTRY]]:
    // CHECK-NEXT: jump [[SUM:bb[0-9]+]]
    // CHECK: [[SUM]]:
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: jumpi [[RECURSE:bb[0-9]+]], [[BASE:bb[0-9]+]]
    // CHECK: [[BASE]]:
    // CHECK-NEXT: pop
    // CHECK: push 0
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump{{$}}
    // CHECK: [[RECURSE]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: sub
    // CHECK-NEXT: push [[RECURSE_RET:bb[0-9]+]]
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump [[SUM_ENTRY]]
    function f(uint256 x) public pure returns (uint256) {
        return sum(x) + sum(x + 1);
    }

    function sum(uint256 x) internal pure returns (uint256) {
        if (x == 0) {
            return 0;
        }
        return x + sum(x - 1);
    }
}
