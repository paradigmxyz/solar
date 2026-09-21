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
    // The recursive call path falls through from the zero test.
    // CHECK: [[SUM]]:
    // CHECK: iszero
    // CHECK-NEXT: push [[BASE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
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
    // Both callers share cleanup that restores the free-memory and frame pointers.
    // CHECK: [[FIRST_RET]] [continuation]:
    // CHECK-NEXT: push [[FIRST_CLEAN:bb[0-9]+]]
    // CHECK-NEXT: jump [[CLEANUP:bb[0-9]+]]
    // CHECK-NEXT: [[CLEANUP]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mstore
    // CHECK: [[SECOND_RET:bb[0-9]+]] [continuation]:
    // CHECK-NEXT: push [[SECOND_CLEAN:bb[0-9]+]]
    // CHECK-NEXT: jump [[CLEANUP]]
    // CHECK: [[RECURSE_RET]] [continuation]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: [[STORE_RESULT:bb[0-9]+]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 96
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump{{$}}
    // The base case shares the result store and stacked return.
    // CHECK: [[BASE]]:
    // CHECK-NEXT: push 0{{$}}
    // CHECK-NEXT: jump [[STORE_RESULT]]
    // CHECK: [[FIRST_CLEAN]] [continuation]:
    // CHECK: push [[SECOND_RET]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[SECOND_CLEAN]] [continuation]:
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
