//@compile-flags: -Zdump=evm-ir
//@ filecheck:

contract WhileEmptyBody {
    // CHECK-LABEL: @module WhileEmptyBody
    // CHECK: push 0xb3de648b
    // CHECK-NEXT: eq
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK: push 32
    // CHECK-NEXT: sgt
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: jump [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]{{.*}}
    // CHECK: calldataload
    // CHECK-NEXT: push [[LOOP]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: stop
    function f(uint256 x) public pure {
        while (x > 0) {}
    }
}
