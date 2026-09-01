//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract Test {
    // CHECK: push 0xc21f7bbb
    // CHECK-NEXT: {{eq|sub}}
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[BODY]]:
    // CHECK: push 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[ONE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[TWO_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: sub
    // CHECK-NEXT: push [[REST:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[ONE]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    // CHECK: [[TWO_BODY]]:
    // CHECK-NEXT: push 2
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: push 128
    // CHECK: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    // CHECK: [[REST]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: sub
    // CHECK: push 4
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    function select(address account, uint256 value) external pure returns (uint256) {
        if (account == address(1)) return value + 1;
        if (account == address(2)) return value + 2;
        if (account == address(3)) return value + 3;
        if (account == address(4)) return value + 4;
        if (account == address(5)) return value + 5;
        return value;
    }
}
