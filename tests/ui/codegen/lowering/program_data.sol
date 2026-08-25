//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

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
}
