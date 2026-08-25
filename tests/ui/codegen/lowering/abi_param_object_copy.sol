//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: memory_object_len memoryarray
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_slice_load_word memory
    // CHECK: memory_object_load_element memoryarray
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
