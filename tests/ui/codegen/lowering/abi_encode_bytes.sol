//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// `abi.encode(...)` stays as a typed MIR operation until the ABI encoding pass,
// which allocates a fresh memory slice before the bytes-object adapter copies
// it into `[length][data...]` memory.
// `keccak256(abi.encode(...))` hashes the encoding staged at the unbumped free
// memory pointer without materializing a `bytes` object.
contract AbiEncodeBytes {
    // CHECK-LABEL: fn @hash3{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = fmp
    // CHECK: mstore [[BASE]], arg0
    // CHECK: {{v[0-9]+}} = keccak256 [[BASE]], 96
    function hash3(uint a, uint b, uint c) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b, c));
    }

    // CHECK-LABEL: fn @encode3{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, word, word], args arg0, arg1, arg2
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[ENCODED]]
    // CHECK: alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, [[LEN]]
    function encode3(uint a, uint b, uint c) external pure returns (bytes memory) {
        return abi.encode(a, b, c);
    }

    // CHECK-LABEL: fn @encodeDynamic{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, memory_bytes], args arg0, arg1
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[ENCODED]]
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, [[LEN]]
    // CHECK: mcopy
    function encodeDynamic(uint a, string memory s) external pure returns (bytes memory) {
        return abi.encode(a, s);
    }

    // CHECK-LABEL: fn @hashDynamic{{[( ]}}
    // CHECK: mcopy
    // CHECK: keccak256
    function hashDynamic(uint a, string memory s) external pure returns (bytes32) {
        return keccak256(abi.encode(a, s));
    }

    // CHECK-LABEL: fn @roundtrip{{[( ]}}
    // CHECK: set_memory_object_len memorybytes
    // CHECK: memory_object_data memorybytes
    // CHECK: mload
    function roundtrip(uint a) external pure returns (uint) {
        return abi.decode(abi.encode(a), (uint));
    }
}
