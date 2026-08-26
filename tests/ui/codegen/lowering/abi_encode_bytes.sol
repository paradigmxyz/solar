//@compile-flags: -O none -Zdump=mir
//@filecheck:

// `abi.encode(...)` stays as a typed MIR operation until the ABI encoding pass,
// which allocates the `[length][data...]` bytes object directly.
// `keccak256(abi.encode(...))` consumes the typed ABI slice directly.
contract AbiEncodeBytes {
    // CHECK-LABEL: fn @hash3{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, word, word], scratch, args arg0, arg1, arg2
    // CHECK: [[DATA:v[0-9]+]] = slice_ptr [[ENCODED]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[ENCODED]]
    // CHECK: {{v[0-9]+}} = keccak256 [[DATA]], [[LEN]]
    function hash3(uint a, uint b, uint c) external pure returns (bytes32) {
        return keccak256(abi.encode(a, b, c));
    }

    // CHECK-LABEL: fn @encode3{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, word, word], object, args arg0, arg1, arg2
    // CHECK: ret [[ENCODED]]
    function encode3(uint a, uint b, uint c) external pure returns (bytes memory) {
        return abi.encode(a, b, c);
    }

    // CHECK-LABEL: fn @encodeDynamic{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, memory_bytes], object, args arg0, arg1
    // CHECK: ret [[ENCODED]]
    function encodeDynamic(uint a, string memory s) external pure returns (bytes memory) {
        return abi.encode(a, s);
    }

    // CHECK-LABEL: fn @hashDynamic{{[( ]}}
    // CHECK: [[ENCODED:v[0-9]+]] = abi_encode [word, memory_bytes], scratch, args arg0, arg1
    // CHECK: [[DATA:v[0-9]+]] = slice_ptr [[ENCODED]]
    // CHECK: [[LEN:v[0-9]+]] = slice_len [[ENCODED]]
    // CHECK: keccak256 [[DATA]], [[LEN]]
    function hashDynamic(uint a, string memory s) external pure returns (bytes32) {
        return keccak256(abi.encode(a, s));
    }

    // CHECK-LABEL: fn @roundtrip{{[( ]}}
    // CHECK: abi_encode [word], object, args arg0
    // CHECK: abi_decode [u256]
    function roundtrip(uint a) external pure returns (uint) {
        return abi.decode(abi.encode(a), (uint));
    }
}
