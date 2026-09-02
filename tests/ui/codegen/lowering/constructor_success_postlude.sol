//@compile-flags: -Zdump=evm-ir
//@filecheck: --enable-var-scope

contract ConstructorSuccessPostlude {
    // CHECK-LABEL: @module ConstructorSuccessPostlude_deployment
    // CHECK: push [[FAIL:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: push [[FAIL]]
    // CHECK-NEXT: jumpi
    // CHECK: codecopy
    // CHECK: return
    // CHECK: [[FAIL]] [cold]:
    // CHECK: revert
    constructor(bool fail) {
        if (fail) revert();
    }
}
