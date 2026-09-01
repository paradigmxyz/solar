//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:
contract AssemblerBlockDedup {
    // CHECK: push 0xdbe671f
    // CHECK: push [[ONE:bb[0-9]+]]
    function a() public pure returns (uint256) {
        return 1;
    }

    // CHECK: push 0x4df7e3d0
    // CHECK: push [[ONE]]
    function b() public pure returns (uint256) {
        return 1;
    }

    // CHECK: push 0x5ce8bda8
    // CHECK: push [[TWO:bb[0-9]+]]
    function c(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }

    // CHECK: push 0xfeb97429
    // CHECK: push [[TWO]]
    // CHECK: [[ONE]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    // CHECK: [[TWO]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: sub
    // CHECK: push 2
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    function d(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }
}
