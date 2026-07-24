//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir --pretty-json
//@ filecheck:

contract ConstructorInternalCallBin {
    uint256 public value;

    // CHECK-LABEL: @module deployment
    // CHECK: push [[CTOR_CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[HELPER:bb[0-9]+]]
    // CHECK: [[HELPER]]:
    // CHECK: push [[RECURSE_BLOCK:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[RECURSE_BLOCK]]:
    // CHECK-NEXT: push 11
    // CHECK: mul
    // CHECK: jumpi
    // CHECK-NEXT: push 1
    // CHECK: push {{bb[0-9]+}}
    // CHECK-NEXT: jump [[HELPER]]
    // CHECK: [[CTOR_CONT]]:
    // CHECK: sstore
    // CHECK: return
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x3fa4f245
    // CHECK: sload
    // CHECK: return
    constructor(uint256 x) {
        value = helper(x & 7);
    }

    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 11 + helper(n - 1);
    }
}
