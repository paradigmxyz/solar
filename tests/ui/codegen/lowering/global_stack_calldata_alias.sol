//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

contract Test {
    // CHECK: push 0xc21f7bbb
    // CHECK: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 1
    // CHECK-NEXT: dup2
    // CHECK-NEXT: eq
    // CHECK: push 2
    // CHECK-NEXT: dup2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[TWO_BODY:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup2
    // CHECK-NEXT: sub
    // CHECK-NEXT: push [[REST:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup3
    // CHECK-NEXT: add
    // CHECK: jump [[STORE:bb[0-9]+]]
    // CHECK: [[STORE]]:
    // CHECK-NEXT: mstore
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    // CHECK: push 1
    // CHECK-NEXT: dup3
    // CHECK-NEXT: add
    // CHECK: jump [[STORE]]
    // CHECK: [[TWO_BODY]]:
    // CHECK: push 2
    // CHECK-NEXT: dup3
    // CHECK-NEXT: add
    // CHECK: jump [[STORE]]
    // CHECK: [[REST]]:
    // CHECK: push 4
    // CHECK: dup3
    // CHECK-NEXT: add
    // CHECK: push 5
    // CHECK: dup2
    // CHECK-NEXT: add
    function select(address account, uint256 value) external pure returns (uint256) {
        if (account == address(1)) return value + 1;
        if (account == address(2)) return value + 2;
        if (account == address(3)) return value + 3;
        if (account == address(4)) return value + 4;
        if (account == address(5)) return value + 5;
        return value;
    }
}
