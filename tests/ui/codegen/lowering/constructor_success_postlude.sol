//@compile-flags: -Zcodegen -Zdump=evm-ir
//@filecheck: --check-prefix=CREATE --enable-var-scope

contract ConstructorSuccessPostlude {
    // CREATE-LABEL: @module deployment
    // CREATE: jumpi
    // CREATE: codecopy
    // CREATE: return
    // CREATE: revert
    constructor(bool fail) {
        if (fail) revert();
    }
}
