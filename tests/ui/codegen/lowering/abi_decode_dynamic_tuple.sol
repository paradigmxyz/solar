//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract AbiDecodeDynamicTuple {
    // CHECK-LABEL: fn @decode
    // CHECK: [[STRING:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[STRING]],
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[BYTES]],
    // CHECK: returndata
    function decode(bytes memory data)
        external
        pure
        returns (uint256 a, string memory s, bytes memory b)
    {
        return abi.decode(data, (uint256, string, bytes));
    }

    // CHECK-LABEL: fn @roundtrip
    // CHECK: set_memory_object_len memorybytes
    // CHECK: mcopy
    // CHECK: set_memory_object_len memorybytes
    // CHECK: returndata
    function roundtrip(uint256 a, string memory s, bytes memory b)
        external
        pure
        returns (uint256, string memory, bytes memory)
    {
        return abi.decode(abi.encode(a, s, b), (uint256, string, bytes));
    }

    // CHECK-LABEL: fn @decodeBytes
    // CHECK: [[INPUT:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[INPUT]],
    // CHECK: [[RESULT:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[RESULT]],
    // CHECK: internal_call @__ret_bytes, 0, [[RESULT]]
    function decodeBytes(bytes memory data) external pure returns (bytes memory) {
        return abi.decode(data, (bytes));
    }
}
