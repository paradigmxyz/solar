//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Packed encoding writes each value's top `size` bytes: fixed-bytes values
// are already left-aligned and must not be shifted again, and `bytes`/
// `string` values copy their data without padding (runtime-length cursor).
contract AbiEncodePackedMixed {
    // CHECK-LABEL: fn @fixedBytesArg{{[( ]}}
    // CHECK: {{v[0-9]+}} = shl 96, arg1
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, arg2
    // CHECK: [[OBJECT:v[0-9]+]] = keccak256_bytes
    function fixedBytesArg(uint a, address b, bytes2 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    // CHECK-LABEL: fn @dynamicArg{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: mcopy {{v[0-9]+}}, {{v[0-9]+}}, [[LEN]]
    // CHECK: [[OBJECT:v[0-9]+]] = keccak256_bytes
    function dynamicArg(bytes32 h, bytes memory tail) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(h, tail));
    }

    // CHECK-LABEL: fn @materialized{{[( ]}}
    // CHECK: mcopy
    // CHECK: [[BOOL:v[0-9]+]] = shl 248, arg2
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, [[BOOL]]
    // CHECK: set_memory_object_len memorybytes
    function materialized(uint16 a, bytes memory mid, bool b) external pure returns (bytes memory) {
        return abi.encodePacked(a, mid, b);
    }
}
