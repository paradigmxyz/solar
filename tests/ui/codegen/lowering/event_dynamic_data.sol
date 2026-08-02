//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventDynamicData {
    event Text(uint256 indexed id, string message, uint256 count);
    event Blob(bytes data);

    // CHECK-LABEL: fn @text{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [memory_bytes, word], args
    // CHECK: [[PTR:v[0-9]+]] = slice_ptr [[ENCODED]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[ENCODED]]
    // CHECK: log2 [[PTR]], [[LEN]], 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 1
    function text(string memory message) external {
        emit Text(1, message, 7);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 5
    // CHECK: mstore {{v[0-9]+}}, 0x736f6c6172000000000000000000000000000000000000000000000000000000
    // CHECK: [[ENCODED2:v[0-9]+]] = abi_encode [memory_bytes, word], args
    // CHECK: [[PTR2:v[0-9]+]] = slice_ptr [[ENCODED2]]
    // CHECK: [[LEN2:v[0-9]+]] = slice_len [[ENCODED2]]
    // CHECK: log2 [[PTR2]], [[LEN2]], 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 2
    function literal() external {
        emit Text(2, "solar", 9);
    }

    // CHECK-LABEL: fn @blob{{[( ]}}
    // CHECK: [[ENCODED3:v[0-9]+]] = abi_encode [memory_bytes], args
    // CHECK: [[PTR3:v[0-9]+]] = slice_ptr [[ENCODED3]]
    // CHECK: [[LEN3:v[0-9]+]] = slice_len [[ENCODED3]]
    // CHECK: log1 [[PTR3]], [[LEN3]], 0xd05ce3dc4caf4a4b252e3323bde615dc3b9d54623e1859c892f0b4ecf5e45164
    function blob(bytes memory data) external {
        emit Blob(data);
    }
}
