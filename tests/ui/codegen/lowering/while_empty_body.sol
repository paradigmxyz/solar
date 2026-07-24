//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir
//@ filecheck:

contract WhileEmptyBody {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0xb3de648b
    // CHECK: jump [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]:
    // CHECK: calldataload
    // CHECK-NEXT: push [[LOOP]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: stop
    function f(uint256 x) public pure {
        while (x > 0) {}
    }
}
