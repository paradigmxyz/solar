//@ revisions: serial parallel
//@ compile-flags: -Zdump=mir -Zmir-pipeline=none
//@[serial] compile-flags: -j1
//@[parallel] compile-flags: -j4
//@ filecheck:

contract SharedLiteralHelpers {
    // CHECK-LABEL: fn @b(
    // CHECK: icall @literal_bytes_1
    function b(uint256 n) external pure returns (bytes memory) {
        if (n == 0) return "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        if (n == 1) return "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        if (n == 2) return "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        return "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    }

    // CHECK-LABEL: fn @a(
    // CHECK: icall @literal_bytes_0
    function a(uint256 n) external pure returns (bytes memory) {
        if (n == 0) return "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        if (n == 1) return "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        if (n == 2) return "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        return "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    }

    // CHECK-LABEL: fn @literal_bytes_1(
    // CHECK: 0x62626262
    // CHECK-LABEL: fn @literal_bytes_0(
    // CHECK: 0x61616161
}
