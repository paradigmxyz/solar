//@ revisions: mir runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@[runtime] compile-flags: -Ogas
//@[runtime] run-call: externalBytesHash() => 0xf1885eda54b7a053318cd41e2093220dab15d65381b1157a3633a83bfd5c9239
//@[runtime] run-call: externalStringHash() => 0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8

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

    function externalBytesHash() external view returns (bytes32) {
        return keccak256(this.bytesLiteral());
    }

    function externalStringHash() external view returns (bytes32) {
        return keccak256(bytes(this.stringLiteral()));
    }
}
