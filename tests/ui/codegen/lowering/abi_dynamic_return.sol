//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract AbiDynamicReturn {
    // CHECK-LABEL: fn @bytesLiteral
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[BYTES]], 3
    // CHECK: mstore {{v[0-9]+}}, 0x102030000000000000000000000000000000000000000000000000000000000
    // CHECK: internal_call @__ret_bytes, 0, [[BYTES]]
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    // CHECK-LABEL: fn @stringLiteral
    // CHECK: [[STRING:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[STRING]], 5
    // CHECK: mstore {{v[0-9]+}}, 0x68656c6c6f000000000000000000000000000000000000000000000000000000
    // CHECK: internal_call @__ret_bytes, 0, [[STRING]]
    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
