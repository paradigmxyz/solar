//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: read() => 1, 2, 3
// ported-from: test/libsolidity/semanticTests/array/copying/array_storage_multi_items_per_slot.sol

contract StoragePackedArrayElements {
    uint8[33] private a;
    uint32[9] private b;
    uint120[3] private c;

    function read() external returns (uint8, uint32, uint120) {
        a[32] = 1;
        a[31] = 2;
        a[30] = 3;
        b[0] = 1;
        b[1] = 2;
        b[2] = 3;
        c[2] = 3;
        c[1] = 1;
        return (a[32], b[1], c[2]);
    }
}
