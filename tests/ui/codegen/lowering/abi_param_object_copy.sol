//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: memory_object_len memoryarray
    // CHECK: icall @store_storage_bytes, 0, 1, arg1
    // CHECK: memory_object_load_element memoryarray
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
