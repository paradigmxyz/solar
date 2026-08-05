//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    // CHECK-LABEL: fn @setText{{[( ]}}
    // CHECK: memory_object_copy_from_slice memorybytes, {{v[0-9]+}}, arg0
    // CHECK: sstore 0,
    function setText(string calldata value) external {
        text = value;
    }

    // CHECK-LABEL: fn @setBlob{{[( ]}}
    // CHECK: memory_object_copy_from_slice memorybytes, {{v[0-9]+}}, arg0
    // CHECK: sstore 1,
    function setBlob(bytes calldata value) external {
        blob = value;
    }

    // CHECK-LABEL: fn @getText{{[( ]}}
    // CHECK: sload 0
    // CHECK: ret
    function getText() external view returns (string memory) {
        return text;
    }

    // CHECK-LABEL: fn @getBlob{{[( ]}}
    // CHECK: sload 1
    // CHECK: ret
    function getBlob() external view returns (bytes memory) {
        return blob;
    }
}
