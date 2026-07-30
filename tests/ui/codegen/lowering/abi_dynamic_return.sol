//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AbiDynamicReturn {
    // CHECK-LABEL: fn @bytesLiteral{{[( ]}}
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 64
    // CHECK: set_memory_object_len memorybytes, [[BYTES]], 3
    // CHECK: mstore {{v[0-9]+}}, 0x102030000000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[BYTES]]
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    // CHECK-LABEL: fn @stringLiteral{{[( ]}}
    // CHECK: [[STRING:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 64
    // CHECK: set_memory_object_len memorybytes, [[STRING]], 5
    // CHECK: mstore {{v[0-9]+}}, 0x68656c6c6f000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[STRING]]
    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
