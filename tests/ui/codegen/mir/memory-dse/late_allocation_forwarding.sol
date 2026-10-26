//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=mir
//@[size] compile-flags: -Osize -Zdump=mir
//@ filecheck:

// The memory-dse after `lower-alloc` runs under both objectives: the second allocation reuses the
// free-memory pointer that the first bump left in a value instead of reloading it, and the log
// reads the encoding from that pointer.
contract LateAllocationForwarding {
    event Data(uint256 value, string text);

    // CHECK-LABEL: fn @emitData
    // CHECK: [[FMP:v[0-9]+]] = mload 64
    // CHECK: [[NEXT:v[0-9]+]] = add [[FMP]], 64
    // CHECK-NOT: mload 64
    // CHECK: log [[NEXT]], 128
    function emitData(uint256 value) external {
        emit Data(value, "hello world");
    }
}
