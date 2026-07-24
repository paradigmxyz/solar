//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@filecheck: --enable-var-scope

contract EmptyExternalReturn {
    // CHECK-LABEL: @module runtime
    // CHECK: callvalue
    // CHECK: revert
    fallback() external {}
}
