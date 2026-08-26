//@ revisions: mir size runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[size] compile-flags: -Osize -Zdump=mir
//@[size] filecheck: --check-prefix=SPLAT
//@[runtime] compile-flags: -Ogas
//@[runtime] run-call: dataHash() => 0xfc1266ee7e93ac2873e7623af26456cf53c18a33ce56a117ef3ef0d901c28394
//@[runtime] run-call: subsliceHash() => 0xfe104a769973081412d46a6d04c990a5e9cc804baf45fa43d99b7dbee24984b8
//@[runtime] run-call: zeroHash() => 0xdfded4ed5ac76ba7379cfe7b3b0f53e768dca8d45a34854e649cfc3c18cbd9cd
//@[runtime] run-call: zeroWordHash() => 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
//@[runtime] run-call: splatHash() => 0x779ba5798a6ad3d608b17a14735a3a2d7d61e8c9817435fc4524dd5d0cf6a177

// CHECK-LABEL: data:
// CHECK: literal_0: hex"
// CHECK: literal_1: hex"
// CHECK: literal_2: hex"
// CHECK-NOT: literal_3:
contract C {
    // CHECK-LABEL: fn @data(
    // CHECK: data_copy literal_0, {{.*}}, 288
    function data() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    // CHECK-LABEL: fn @subslice(
    // CHECK: data_copy literal_1, {{.*}}, 192
    function subslice() external pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ";
    }

    function dataHash() external pure returns (bytes32) {
        return keccak256("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!");
    }

    function subsliceHash() external pure returns (bytes32) {
        return keccak256("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ");
    }

    // CHECK-LABEL: fn @zeroData{{[.(]}}
    // CHECK: memory_zero {{.*}}, 160
    function zeroData() public pure returns (bytes memory) {
        return hex"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    }

    // CHECK-LABEL: fn @zeroWord{{[.(]}}
    // CHECK: mstore {{.*}}, 0
    function zeroWord() public pure returns (bytes memory) {
        return hex"0000000000000000000000000000000000000000000000000000000000000000";
    }

    // CHECK-LABEL: fn @splatData{{[.(]}}
    // CHECK: data_copy literal_2, {{.*}}, 160
    // SPLAT-LABEL: fn @splatData.{{[0-9]+}}() -> memptr
    // SPLAT: mstore v1, 0x112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00
    // SPLAT: mcopy {{.*}}, 32
    // SPLAT: mcopy {{.*}}, 64
    // SPLAT: mcopy {{.*}}, 32
    function splatData() public pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00";
    }

    function zeroHash() external pure returns (bytes32) {
        return keccak256(zeroData());
    }

    function zeroWordHash() external pure returns (bytes32) {
        return keccak256(zeroWord());
    }

    function splatHash() external pure returns (bytes32) {
        return keccak256(splatData());
    }
}
