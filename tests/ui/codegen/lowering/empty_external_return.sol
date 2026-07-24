//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@filecheck: --check-prefix=RUNTIME --enable-var-scope

contract EmptyExternalReturn {
    // RUNTIME-LABEL: @module runtime
    // RUNTIME: callvalue
    // RUNTIME: revert
    fallback() external {}
}
