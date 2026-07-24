//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

contract InternalCallFrameDealloc {
    // CHECK: push 0xb3de648b
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 224
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[FIRST_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM:bb[0-9]+]]
    // CHECK: [[SUM]]:
    // CHECK: [[FIRST_RET]]:
    // CHECK: push [[FIRST_BODY:bb[0-9]+]]
    // CHECK-NEXT: jump [[DEALLOC:bb[0-9]+]]
    // CHECK: [[DEALLOC]]:
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: [[SECOND_RET:bb[0-9]+]]:
    // CHECK: push [[FINAL:bb[0-9]+]]
    // CHECK-NEXT: jump [[DEALLOC]]
    // CHECK: push 1
    // CHECK: push [[RECURSE_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[RECURSE_RET]]:
    // CHECK: jump [[DEALLOC]]
    // CHECK: [[FIRST_BODY]]:
    // CHECK: push [[SECOND_RET]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[FINAL]]:
    // CHECK: add
    // CHECK: return
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
