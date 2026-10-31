//@ revisions: constructor runtime mir
//@[constructor] compile-flags: -Ogas -Zdump=evm-ir
//@[constructor] filecheck: --check-prefix=CONSTRUCTOR
//@[runtime] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR

// MIR-LABEL: @data
// MIR: literal_0: hex"aaaaaaaa
// MIR: literal_1: hex"11111111
// CONSTRUCTOR-LABEL: @module C_deployment
// CONSTRUCTOR: literal_0: hex"aaaaaaaa
// CONSTRUCTOR-NOT: literal_1
// RUNTIME-LABEL: @module C_runtime
// RUNTIME: literal_0: hex"11111111
// RUNTIME-NOT: aaaaaaaa
contract C {
    event ConstructorData(bytes data);

    constructor() {
        emit ConstructorData(hex"aaaaaaaace0327b4a7ee880e1b2f19349fdf64a637c5a6aabec3677586e2ee47eb16b958898642b9087095abd1cc23f7c2725c1573c4d6f2afbccb10ccc8a8555f0c3e87f69d113711bc1a5105cc9bfa756df5631a37a9b46e91518e1a20ea885327475367468a930e43266a2eecab51e766985012e000578257a68e89e7b70eac94f31399cd36e54999d3951bf8e2b924c312e3627ead583a4e0415cf329277e772705ab8a70bb20723f831cfb640e7e7a572d8070c989c8fd6f49bc52976fad96580d3a91320d805ecafe9362dee89adb1907f264cfc1f986bbc96935d4eab");
    }

    function runtimeData() external pure returns (bytes memory) {
        return hex"1111111100952be69a73dee295a654c12c959ba775d27501e391523a0b96c19a38ed4e631a5376ad727d65213a79f3108541cf95012969a0d3064f108b5dd6e7f8c19b89ab30fe93399787ab66a0dc0d678c575209b1bd9b068282cec843c86f5d6ce8b68260ec8ef8e01e854aab2c2437dd0aae47b8401d4f5fb0620dc7b78023987e313ec4438ca5277fefe94a97f2aace1e7dee55f3817a80b9161cb2860c";
    }
}
