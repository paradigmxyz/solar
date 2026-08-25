//@ revisions: mir runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[runtime] compile-flags: -Ogas
//@[runtime] run-call: dataHash() => 0xfc1266ee7e93ac2873e7623af26456cf53c18a33ce56a117ef3ef0d901c28394
//@[runtime] run-call: subsliceHash() => 0xfe104a769973081412d46a6d04c990a5e9cc804baf45fa43d99b7dbee24984b8

// CHECK-LABEL: data:
// CHECK: 0: hex"
// CHECK-NOT: 1: hex"
// CHECK: data_copy literal_0,
// CHECK: data_copy literal_0+64,
// CHECK: memory_zero
// CHECK: mcopy
contract C {
    function data() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    function subslice() external pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ";
    }

    function dataHash() external pure returns (bytes32) {
        return keccak256("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!");
    }

    function subsliceHash() external pure returns (bytes32) {
        return keccak256("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ");
    }

    function zeroData() external pure returns (bytes memory) {
        return hex"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    }

    function splatData() external pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00";
    }
}
