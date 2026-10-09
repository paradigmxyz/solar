//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    // CHECK-LABEL: fn @setText{{[( ]}}
    // CHECK: [[TEXT_VIEW:v[0-9]+]] = memory_slice [[TEXT:v[0-9]+]]
    // CHECK-NEXT: slice_copy [[TEXT_VIEW]], 0, arg0
    // CHECK: icall @store_storage_bytes, 0, [[TEXT]]
    function setText(string calldata value) external {
        text = value;
    }

    // CHECK-LABEL: fn @setBlob{{[( ]}}
    // CHECK: [[BLOB_VIEW:v[0-9]+]] = memory_slice [[BLOB:v[0-9]+]]
    // CHECK-NEXT: slice_copy [[BLOB_VIEW]], 0, arg0
    // CHECK: icall @store_storage_bytes, 1, [[BLOB]]
    function setBlob(bytes calldata value) external {
        blob = value;
    }

    // CHECK-LABEL: fn @getText{{[( ]}}
    // CHECK: {{v[0-9]+}} = icall @load_storage_bytes{{.*}}, 0
    // CHECK: ret
    function getText() external view returns (string memory) {
        return text;
    }

    // CHECK-LABEL: fn @getBlob{{[( ]}}
    // CHECK: {{v[0-9]+}} = icall @load_storage_bytes{{.*}}, 1
    // CHECK: ret
    function getBlob() external view returns (bytes memory) {
        return blob;
    }
}
