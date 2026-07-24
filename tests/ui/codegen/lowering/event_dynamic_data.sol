//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract EventDynamicData {
    event Text(uint256 indexed id, string message, uint256 count);
    event Blob(bytes data);

    // CHECK-LABEL: fn @text
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: mcopy {{v[0-9]+}}, {{v[0-9]+}}, [[LEN]]
    // CHECK: log2 {{v[0-9]+}}, {{v[0-9]+}}, 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 1
    function text(string memory message) external {
        emit Text(1, message, 7);
    }

    // CHECK-LABEL: fn @literal
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 5
    // CHECK: mstore {{v[0-9]+}}, 0x736f6c6172000000000000000000000000000000000000000000000000000000
    // CHECK: log2 {{v[0-9]+}}, {{v[0-9]+}}, 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 2
    function literal() external {
        emit Text(2, "solar", 9);
    }

    // CHECK-LABEL: fn @blob
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: mcopy {{v[0-9]+}}, {{v[0-9]+}}, [[LEN]]
    // CHECK: log1 {{v[0-9]+}}, {{v[0-9]+}}, 0xd05ce3dc4caf4a4b252e3323bde615dc3b9d54623e1859c892f0b4ecf5e45164
    function blob(bytes memory data) external {
        emit Blob(data);
    }
}
