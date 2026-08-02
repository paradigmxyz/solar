//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    // CHECK-LABEL: fn @setText{{[( ]}}
    // CHECK: memory_object_copy_from_slice memorybytes, {{v[0-9]+}}, arg0
    // CHECK: sload 0
    // CHECK: sstore 0,
    function setText(string calldata value) external {
        text = value;
    }

    // CHECK-LABEL: fn @setBlob{{[( ]}}
    // CHECK: memory_object_copy_from_slice memorybytes, {{v[0-9]+}}, arg0
    // CHECK: sload 1
    // CHECK: sstore 1,
    function setBlob(bytes calldata value) external {
        blob = value;
    }

    // CHECK-LABEL: fn @getText{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: ret [[VALUE]]
    function getText() external view returns (string memory) {
        return text;
    }

    // CHECK-LABEL: fn @getBlob{{[( ]}}
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 1
    // CHECK: ret [[VALUE]]
    function getBlob() external view returns (bytes memory) {
        return blob;
    }
}
