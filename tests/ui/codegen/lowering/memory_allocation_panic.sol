//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Dynamic memory allocations must revert with Panic(0x41) when the requested
// size overflows while computing the padded byte length, element byte length,
// total allocation size, or free-memory pointer bump.
contract MemoryAllocationPanic {
    // CHECK-LABEL: fn @makeBytes{{[( ]}}
    // CHECK: [[PADDED:v[0-9]+]] = add arg0, 63
    // CHECK: [[PADDED_OVERFLOW:v[0-9]+]] = lt [[PADDED]], arg0
    // CHECK: [[MASK:v[0-9]+]] = not 31
    // CHECK: [[BYTES:v[0-9]+]] = and [[PADDED]], [[MASK]]
    // CHECK: alloc memorybytes, exact, zeroed, panic, [[BYTES]]
    // CHECK: mstore 4, 65
    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    // CHECK-LABEL: fn @makeArray{{[( ]}}
    // CHECK: [[ELEMENTS:v[0-9]+]] = mul arg0, 1
    // CHECK: [[TOTAL:v[0-9]+]] = add [[ELEMENTS]], 1
    // CHECK: [[BYTES:v[0-9]+]] = mul [[TOTAL]], 32
    // CHECK: alloc memoryarray<1>, exact, zeroed, panic, [[BYTES]]
    // CHECK: mstore 4, 65
    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
