//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@filecheck: --enable-var-scope

contract EmptyExternalReturn {
    // CHECK-LABEL: @module runtime
    // CHECK: callvalue
    // CHECK-NOT: calldatasize
    // CHECK-NOT: calldataload
    // CHECK: stop
    fallback() external {}
}
