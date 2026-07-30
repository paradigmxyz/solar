//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime --pretty-json
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
    // CHECK: push 1
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    // CHECK: [[TWO]]:
    // CHECK: push {{bb[0-9]+}}
    // CHECK: jumpi
    // CHECK: push 2
    // CHECK: jump [[RETURN]]
    function d(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }
}
