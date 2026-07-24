//@compile-flags: -Zcodegen -Zdump=evm-ir
//@filecheck: --enable-var-scope

contract ConstructorSuccessPostlude {
    // CHECK-LABEL: @module deployment
    // CHECK: jumpi
    // CHECK: codecopy
    // CHECK: return
    // CHECK: revert
    constructor(bool fail) {
        if (fail) revert();
    }
}
