//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Finalize the complete return prefix before initializing named return
// variables. Otherwise the first initializer in a two-return internal helper
// lands one word before the slot used by the body, and a reused static frame
// leaks the previous call's value.
contract MultiReturnNamedInit {
    // CHECK-LABEL: fn @readU64{{[( ]}}
    // CHECK: [[VALUE_SLOT:v[0-9]+]] = internal_frame_addr 192
    // CHECK-NEXT: mstore [[VALUE_SLOT]], 0
    function readU64(bytes calldata data, uint256 start)
        internal
        pure
        returns (uint64 value, uint256 offset)
    {
        offset = start;
        for (uint256 i = 0; i < 8; i++) {
            value <<= 8;
            value |= uint8(data[offset]);
            offset++;
        }
    }

    // CHECK-LABEL: fn @readTwice{{[( ]}}
    // CHECK: internal_call @readU64, 2, arg0, 0
    // CHECK: internal_call @readU64, 2, arg0, 8
    function readTwice(bytes calldata data) external pure returns (uint64 first, uint64 second) {
        (first,) = readU64(data, 0);
        (second,) = readU64(data, 8);
    }
}
