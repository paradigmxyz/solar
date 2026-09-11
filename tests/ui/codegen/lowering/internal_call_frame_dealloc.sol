//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

contract ICallFrameDealloc {
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
    // The recursive call body precedes its test, giving the call edge a fallthrough.
    // CHECK: [[RECURSE:bb[0-9]+]]:
    // CHECK-NEXT: push 1{{$}}
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 192
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: pop
    // CHECK-NEXT: push [[RECURSE_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[SUM]]:
    // CHECK: push [[RECURSE]]
    // CHECK-NEXT: jumpi
    // The base case stores its result into the frame and returns through the stacked address.
    // CHECK-NEXT: push 0{{$}}
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 96
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump{{$}}
    // Each continuation reads the result, then releases the callee frame by resetting the
    // free memory pointer to the frame base before restoring the caller frame.
    // CHECK: [[FIRST_RET]] [continuation]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: push 1{{$}}
    // CHECK: push 224
    // CHECK-NEXT: mstore
    // CHECK: push 192
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[SECOND_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[SECOND_RET]] [continuation]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: return
    // CHECK: [[RECURSE_RET]] [continuation]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: push 96
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump{{$}}
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
