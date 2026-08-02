//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: memory_object_copy_from_slice memoryarray
    // CHECK: memory_object_copy_from_slice memorybytes
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
