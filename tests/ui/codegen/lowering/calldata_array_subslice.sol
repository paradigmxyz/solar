//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract CalldataArraySubslice {
    // A sub-slice of a word-element calldata array materializes correctly: the
    // slice value carries the data pointer and length, so a word copy from the
    // adjusted position rebuilds the memory array.
    // CHECK-LABEL: fn @word{{[( ]}}
    // CHECK: memory_object_copy_from_slice memoryarray
    function word(uint256[] calldata a) external pure returns (uint256[] memory) {
        return a[1:];
    }
}
