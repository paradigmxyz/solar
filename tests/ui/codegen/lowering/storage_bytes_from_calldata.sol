//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    // CHECK-LABEL: fn @setText{{[( ]}}
    // CHECK: [[TEXT:v[0-9]+]] = alloc memorybytes
    // CHECK: [[TEXT_DATA:v[0-9]+]] = ptradd [[TEXT]], 32
    // CHECK-NEXT: [[SRC:v[0-9]+]] = slice_ptr arg0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK-NEXT: copy calldata [[TEXT_DATA]], [[SRC]], [[LEN]]
    // CHECK: icall @store_storage_bytes, 0, [[TEXT]]
    function setText(string calldata value) external {
        text = value;
    }

    // CHECK-LABEL: fn @setBlob{{[( ]}}
    // CHECK: [[BLOB:v[0-9]+]] = alloc memorybytes
    // CHECK: [[BLOB_DATA:v[0-9]+]] = ptradd [[BLOB]], 32
    // CHECK-NEXT: [[SRC:v[0-9]+]] = slice_ptr arg0
    // CHECK-NEXT: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK-NEXT: copy calldata [[BLOB_DATA]], [[SRC]], [[LEN]]
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
