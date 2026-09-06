//@compile-flags: -Zdump=evm-ir
//@filecheck: --enable-var-scope

contract ConstructorSuccessPostlude {
    // CHECK-LABEL: @module ConstructorSuccessPostlude_deployment
    // CHECK: callvalue
    // CHECK-NEXT: jumpi [[FAIL:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: gt
    // CHECK-NEXT: jumpi [[FAIL]], {{bb[0-9]+}}
    // CHECK: [[FAIL]]:
    // CHECK: revert
    // CHECK: jumpi [[FAIL]], [[SUCCESS:bb[0-9]+]]
    // CHECK: [[SUCCESS]]:
    // CHECK: codecopy
    // CHECK: return
    constructor(bool fail) {
        if (fail) revert();
    }
}
