//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiParamObjectCopy {
    uint256[] public storedWords;
    bytes public storedBytes;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = mload arg0
    // CHECK: sstore 0, [[LEN]]
    // CHECK: icall @store_storage_bytes, 1, arg1
    // CHECK: [[ELEM:v[0-9]+]] = ptradd arg0, {{v[0-9]+}}
    // CHECK-NEXT: {{v[0-9]+}} = mload [[ELEM]]+32
    constructor(uint256[] memory words, bytes memory data) {
        storedWords = words;
        storedBytes = data;
    }
}
