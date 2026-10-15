//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AbiDynamicReturn {
    // CHECK-LABEL: fn @bytesLiteral{{[( ]}}
    // CHECK: [[BYTES:v[0-9]+]] = alloc 64
    // CHECK: mstore [[BYTES]], 3
    // CHECK: [[BYTES_32:v[0-9]+]] = ptradd [[BYTES]], 32
    // CHECK-NEXT: mstore [[BYTES_32]], 0x102030000000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[BYTES]]
    function bytesLiteral() public pure returns (bytes memory) {
        return hex"010203";
    }

    // CHECK-LABEL: fn @stringLiteral{{[( ]}}
    // CHECK: [[STRING:v[0-9]+]] = alloc 64
    // CHECK: mstore [[STRING]], 5
    // CHECK: [[STRING_32:v[0-9]+]] = ptradd [[STRING]], 32
    // CHECK-NEXT: mstore [[STRING_32]], 0x68656c6c6f000000000000000000000000000000000000000000000000000000
    // CHECK: ret [[STRING]]
    function stringLiteral() public pure returns (string memory) {
        return "hello";
    }
}
