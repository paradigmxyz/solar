//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=mir
//@[size] compile-flags: -Osize -Zdump=mir
//@ filecheck:

// Literal ABI lowering removes the temporary string allocation. Under both objectives the
// encoder writes all four event words directly at the heap floor without reloading the FMP.
contract LateAllocationForwarding {
    event Data(uint256 value, string text);

    // CHECK-LABEL: fn @emitData
    // CHECK: [[FLOOR:v[0-9]+]] = heap_floor
    // CHECK-NEXT: [[TAIL:v[0-9]+]] = heap_floor_offset 64
    // CHECK-NEXT: mstore [[FLOOR]], arg0
    // CHECK-NEXT: [[OFFSET:v[0-9]+]] = heap_floor_offset 32
    // CHECK-NEXT: mstore [[OFFSET]], 64
    // CHECK-NEXT: mstore [[TAIL]], 11
    // CHECK-NEXT: [[DATA:v[0-9]+]] = heap_floor_offset 96
    // CHECK-NEXT: mstore [[DATA]], 0x68656c6c6f20776f726c64000000000000000000000000000000000000000000
    // CHECK-NOT: mload 64
    // CHECK-NOT: mcopy
    // CHECK-NEXT: log [[FLOOR]], 128
    function emitData(uint256 value) external {
        emit Data(value, "hello world");
    }
}
