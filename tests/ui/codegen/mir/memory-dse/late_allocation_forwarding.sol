//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=mir
//@[size] compile-flags: -Osize -Zdump=mir
//@ filecheck:

// The memory-dse after `lower-alloc` runs under both objectives: the second allocation reuses the
// free-memory pointer that the first bump left in a value, so the reload and the pointer store
// go, and the log reads the encoding from the forwarded pointer.
contract LateAllocationForwarding {
    event Data(uint256 value, string text);

    // CHECK-LABEL: fn @emitData
    // CHECK: {{v[0-9]+}} = mload 64
    // CHECK-NOT: mload 64
    // CHECK-NOT: mstore 64
    // CHECK: log
    function emitData(uint256 value) external {
        emit Data(value, "hello world");
    }
}
