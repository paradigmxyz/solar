//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@filecheck: --enable-var-scope

contract EmptyExternalReturn {
    // CHECK-LABEL: @module runtime
    // CHECK: calldatasize
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: jump [[SUCCESS:bb[0-9]+]]
    // CHECK: [[SUCCESS]]:
    // CHECK-NEXT: stop
    fallback() external {}
}
