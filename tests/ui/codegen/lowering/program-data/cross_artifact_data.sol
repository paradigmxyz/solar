//@ revisions: constructor runtime mir
//@[constructor] compile-flags: -Ogas -Zdump=evm-ir
//@[constructor] filecheck: --check-prefix=CONSTRUCTOR
//@[runtime] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR

// MIR-LABEL: data:
// MIR: literal_0: hex"aaaaaaaa
// MIR: literal_1: hex"11111111
// CONSTRUCTOR-LABEL: @module C_deployment
// CONSTRUCTOR: @data literal_0 hex"aaaaaaaa
// CONSTRUCTOR-NOT: @data literal_1
// RUNTIME-LABEL: @module C_runtime
// RUNTIME: @data literal_0 hex"11111111
// RUNTIME-NOT: aaaaaaaa
contract C {
    event ConstructorData(bytes data);

    constructor() {
        emit ConstructorData(hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    }

    function runtimeData() external pure returns (bytes memory) {
        return hex"11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    }
}
