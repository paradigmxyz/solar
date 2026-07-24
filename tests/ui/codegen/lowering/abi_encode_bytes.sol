//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// `abi.encode(...)` allocates a fresh `bytes memory` `[length][data...]` from
// the free memory pointer; it must never stage argument words at absolute low
// memory (which clobbers the free memory pointer at 0x40 with 3+ words).
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
    // CHECK: [[ENCODED:v[0-9]+]] = fmp
    // CHECK: [[DATA:v[0-9]+]] = memory_object_data memorybytes, [[ENCODED]]
    // CHECK: mstore [[DATA]], arg0
    // CHECK: set_memory_object_len memorybytes, [[ENCODED]], 96
    function encode3(uint a, uint b, uint c) external pure returns (bytes memory) {
        return abi.encode(a, b, c);
    }

    // CHECK-LABEL: fn @encodeDynamic{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = fmp
    // CHECK: mcopy
    // CHECK: set_memory_object_len memorybytes, [[ENCODED]],
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
