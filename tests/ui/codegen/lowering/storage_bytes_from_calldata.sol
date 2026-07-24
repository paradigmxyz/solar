//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract StorageBytesFromCalldata {
    string text;
    bytes blob;

    // CHECK-LABEL: fn @setText
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr arg0
    // CHECK: calldatacopy {{v[0-9]+}}, [[PTR]], [[LEN]]
    // CHECK: sload 0
    // CHECK: sstore 0,
    function setText(string calldata value) external {
        text = value;
    }

    // CHECK-LABEL: fn @setBlob
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr arg0
    // CHECK: calldatacopy {{v[0-9]+}}, [[PTR]], [[LEN]]
    // CHECK: sload 1
    // CHECK: sstore 1,
    function setBlob(bytes calldata value) external {
        blob = value;
    }

    // CHECK-LABEL: fn @getText
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 0
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function getText() external view returns (string memory) {
        return text;
    }

    // CHECK-LABEL: fn @getBlob
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @__load_storage_bytes, 1, 1
    // CHECK: internal_call @__ret_bytes, 0, [[VALUE]]
    function getBlob() external view returns (bytes memory) {
        return blob;
    }
}
