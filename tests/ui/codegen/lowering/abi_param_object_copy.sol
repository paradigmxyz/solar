//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @_anonymous{{.*abi_args=lazy.*}}
    // CHECK: memory_object_len memoryarray
    // CHECK: memory_object_load_element memoryarray
    // CHECK: memory_object_len memorybytes
    // CHECK: memory_slice_load_word memory
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
