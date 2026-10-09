//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: [[WORDS:v[0-9]+]] = memory_slice arg0
    // CHECK: slice_len [[WORDS]]
    // CHECK: icall @store_storage_bytes, 1, arg1
    // CHECK: [[WORDS_VIEW:v[0-9]+]] = memory_slice arg0
    // CHECK: slice_load_element [[WORDS_VIEW]]
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
