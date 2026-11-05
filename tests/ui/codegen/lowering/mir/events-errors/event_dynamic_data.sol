//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract EventDynamicData {
    event Text(uint256 indexed id, string message, uint256 count);
    event Blob(bytes data);

    // CHECK-LABEL: fn @text{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [memory_bytes, word], scratch, args
    // CHECK: [[PTR:v[0-9]+]] = extract_value slice<memory>, [[ENCODED]], 0
    // CHECK: [[LEN:v[0-9]+]] = extract_value slice<memory>, [[ENCODED]], 1
    // CHECK: log [[PTR]], [[LEN]], 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 1
    function text(string memory message) external {
        emit Text(1, message, 7);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[LITERAL:v[0-9]+]] = alloc
    // CHECK: mstore [[LITERAL]], 5
    // CHECK: [[LITERAL_32:v[0-9]+]] = ptradd [[LITERAL]], 32
    // CHECK-NEXT: mstore [[LITERAL_32]], 0x736f6c6172000000000000000000000000000000000000000000000000000000
    // CHECK: [[ENCODED2:v[0-9]+]] = abi_encode [memory_bytes, word], scratch, args
    // CHECK: [[PTR2:v[0-9]+]] = extract_value slice<memory>, [[ENCODED2]], 0
    // CHECK: [[LEN2:v[0-9]+]] = extract_value slice<memory>, [[ENCODED2]], 1
    // CHECK: log [[PTR2]], [[LEN2]], 0x1ec47f6be8a8bf4aa7aa1659aceb7cef3b607892101a00e4afd57e2ae4fbf3c4, 2
    function literal() external {
        emit Text(2, "solar", 9);
    }

    // CHECK-LABEL: fn @blob{{[( ]}}
    // CHECK: [[ENCODED3:v[0-9]+]] = abi_encode [memory_bytes], scratch, args
    // CHECK: [[PTR3:v[0-9]+]] = extract_value slice<memory>, [[ENCODED3]], 0
    // CHECK: [[LEN3:v[0-9]+]] = extract_value slice<memory>, [[ENCODED3]], 1
    // CHECK: log [[PTR3]], [[LEN3]], 0xd05ce3dc4caf4a4b252e3323bde615dc3b9d54623e1859c892f0b4ecf5e45164
    function blob(bytes memory data) external {
        emit Blob(data);
    }
}
