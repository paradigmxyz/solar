//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDynamicReturn {
    // CHECK-LABEL: fn @bytesLiteral{{[( ]}}
    // CHECK: [[BYTES:v[0-9]+]] = alloc 64
    // CHECK: store [[BYTES]], 3
    // CHECK: [[BYTES_BASE:v[0-9]+]] = ptrtoint ptr<memory> [[BYTES]] to i256
    // CHECK: [[BYTES_DATA:v[0-9]+]] = add [[BYTES_BASE]], 32
    // CHECK: store [[BYTES_DATA]], 0x102030000000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[BYTES]]
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    // CHECK-LABEL: fn @stringLiteral{{[( ]}}
    // CHECK: [[STRING:v[0-9]+]] = alloc 64
    // CHECK: store [[STRING]], 5
    // CHECK: [[STRING_BASE:v[0-9]+]] = ptrtoint ptr<memory> [[STRING]] to i256
    // CHECK: [[STRING_DATA:v[0-9]+]] = add [[STRING_BASE]], 32
    // CHECK: store [[STRING_DATA]], 0x68656c6c6f000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[STRING]]
    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
