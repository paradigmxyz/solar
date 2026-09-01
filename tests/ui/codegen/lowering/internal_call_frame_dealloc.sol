//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

contract InternalCallFrameDealloc {
    // CHECK: push 0xb3de648b
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 192
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[FIRST_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM:bb[0-9]+]]
    // CHECK: [[SUM]]:
    // CHECK: [[FIRST_RET]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push [[EPILOGUE_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[EPILOGUE:bb[0-9]+]]
    // CHECK: [[EPILOGUE]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push 1
    // CHECK: push 192
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: push [[RECURSE_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[RECURSE_RET]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push [[SECOND_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[EPILOGUE]]
    // CHECK: [[SECOND_RET]]:
    // CHECK: push 160
    // CHECK-NEXT: mload
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
