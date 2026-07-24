//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Dynamic memory allocations must revert with Panic(0x41) when the requested
// size overflows while computing the padded byte length, element byte length,
// total allocation size, or free-memory pointer bump.
contract MemoryAllocationPanic {
    // CHECK-LABEL: fn @makeBytes
    // CHECK: [[PADDED:v[0-9]+]] = add arg0, 31
    // CHECK: lt [[PADDED]], arg0
    // CHECK: mstore 4, 65
    // CHECK: [[TOTAL:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK: mstore 4, 65
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes, exact, zeroed, panic, [[TOTAL]]
    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    // CHECK-LABEL: fn @makeArray
    // CHECK: [[BYTES:v[0-9]+]] = mul arg0, 32
    // CHECK: mstore 4, 65
    // CHECK: [[TOTAL:v[0-9]+]] = add [[BYTES]], 32
    // CHECK: mstore 4, 65
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryarray<1>, exact, zeroed, panic, [[TOTAL]]
    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
