//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Dynamic memory allocations must revert with Panic(0x41) when the requested
// size overflows while computing the padded byte length, element byte length,
// total allocation size, or free-memory pointer bump.
contract MemoryAllocationPanic {
    // CHECK-LABEL: fn @makeBytes{{[( ]}}
    // CHECK: [[ROUNDED:v[0-9]+]] = add arg0, 31
    // CHECK: [[MASK:v[0-9]+]] = not 31
    // CHECK: [[PADDED:v[0-9]+]] = and [[ROUNDED]], [[MASK]]
    // CHECK: [[BYTES:v[0-9]+]] = add [[PADDED]], 32
    // CHECK: [[BYTES_OVERFLOW:v[0-9]+]] = lt [[BYTES]], arg0
    // CHECK: icall panic_if<0x41>, [[BYTES_OVERFLOW]]
    // CHECK: alloc zeroed checked [[BYTES]]
    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    // CHECK-LABEL: fn @makeArray{{[( ]}}
    // CHECK: [[ELEMENTS:v[0-9]+]] = mul arg0, 1
    // CHECK: [[TOTAL:v[0-9]+]] = add [[ELEMENTS]], 1
    // CHECK: [[BYTES:v[0-9]+]] = mul [[TOTAL]], 32
    // CHECK: icall panic_if<0x41>, {{v[0-9]+}}
    // CHECK: alloc zeroed checked [[BYTES]]
    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
