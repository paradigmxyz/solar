//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

contract Test {
    // CHECK: push 0xc21f7bbb
    // CHECK-NEXT: sub
    // CHECK: push 2
    // CHECK-NEXT: dup2
    // CHECK-NEXT: eq
    // CHECK: push {{bb[0-9]+}}
    // CHECK: jumpi
    // CHECK: push 3
    // CHECK: dup2
    // CHECK: sub
    // CHECK: push [[REST:bb[0-9]+]]
    // CHECK: jumpi
    // CHECK: push 3
    // CHECK: jump {{bb[0-9]+}}
    // CHECK: [[REST]]:
    // CHECK: push 4
    // CHECK-NEXT: dup2
    // CHECK-NEXT: sub
    // CHECK: push 4
    // CHECK-NEXT: jump {{bb[0-9]+}}
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
