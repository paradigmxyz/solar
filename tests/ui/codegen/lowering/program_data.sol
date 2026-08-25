//@ revisions: mir runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[runtime] compile-flags: -Ogas
//@[runtime] run-call: dataHash() => 0x73c69ad95474a1cf9bd0a9f17079706471526572f19ebe5455e59e5cfcaaaa7c
//@[runtime] run-call: subsliceHash() => 0xa4107d75c07c24b2f5f13e1bb9844c8fb2e073e100384973a6adb918d90bca2d

// CHECK-LABEL: data:
// CHECK: 0: hex"
// CHECK-NOT: 1: hex"
// CHECK: data_copy literal_0,
// CHECK: data_copy literal_0+64,
contract C {
    function data() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";
    }

    function subslice() external pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    }

    function dataHash() external pure returns (bytes32) {
        return keccak256("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ");
    }

    function subsliceHash() external pure returns (bytes32) {
        return keccak256("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    }
}
