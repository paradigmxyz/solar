//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck: --implicit-check-not=mload
//@ run-call: f 5 => 36
//@ run-call: f 0 => 1

// Recursive calls keep the argument, the partial sum and the return address on
// the stack. No call reserves or releases a frame through the free memory
// pointer, so nothing reads memory back.
contract ICallFrameDealloc {
    // CHECK: push 0xb3de648b
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: calldataload
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push [[FIRST_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM:bb[0-9]+]]
    // The recursive call path falls through from the zero test.
    // CHECK: [[SUM]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push [[BASE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 1{{$}}
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: sub
    // CHECK-NEXT: push [[RECURSE_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[FIRST_RET]] [continuation]:
    // CHECK: jumpi
    // CHECK-NEXT: push [[SECOND_RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[SUM]]
    // CHECK: [[SECOND_RET]] [continuation]:
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
