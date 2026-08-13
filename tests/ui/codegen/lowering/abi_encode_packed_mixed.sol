//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Packed encoding writes each value's top `size` bytes: fixed-bytes values
// are already left-aligned and must not be shifted again, and `bytes`/
// `string` values copy their data without padding (runtime-length cursor).
contract AbiEncodePackedMixed {
    // CHECK-LABEL: fn @fixedBytesArg{{[( ]}}
    // CHECK: {{v[0-9]+}} = and arg1, {{.*}}
    // CHECK: {{v[0-9]+}} = shl 96, {{v[0-9]+}}
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, {{v[0-9]+}}
    // CHECK: [[OBJECT:v[0-9]+]] = keccak256_bytes
    function fixedBytesArg(uint a, address b, bytes2 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    // CHECK-LABEL: fn @dynamicArg{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memorybytes
    // CHECK: memory_object_copy_from_slice_at memorybytes
    // CHECK: [[OBJECT:v[0-9]+]] = keccak256_bytes
    function dynamicArg(bytes32 h, bytes memory tail) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(h, tail));
    }

    // CHECK-LABEL: fn @materialized{{[( ]}}
    // CHECK: set_memory_object_len memorybytes
    // CHECK: {{v[0-9]+}} = shl 240, {{v[0-9]+}}
    // CHECK: memory_object_copy_from_slice_at memorybytes
    // CHECK: [[BOOL:v[0-9]+]] = shl 248, {{v[0-9]+}}
    // CHECK: memory_object_store_word memorybytes, {{.*}}, {{.*}}, [[BOOL]]
    function materialized(uint16 a, bytes memory mid, bool b) external pure returns (bytes memory) {
        return abi.encodePacked(a, mid, b);
    }

    // CHECK-LABEL: fn @hashArray{{[( ]}}
    // CHECK: memory_object_load_element memoryarray<1>
    // CHECK: memory_object_store_word memorybytes
    function hashArray(bytes32[] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    // Signed packed values are sign-extended words. Coalescing them with a
    // preceding field must mask the sign extension before shifting, or the
    // high bits overwrite that field.
    // CHECK-LABEL: fn @signedStaticRun{{[( ]}}
    // CHECK: [[CLEAN:v[0-9]+]] = and {{v[0-9]+}}, 0xffff
    // CHECK: [[SIGNED:v[0-9]+]] = shl 232, [[CLEAN]]
    // CHECK: keccak256_bytes
    function signedStaticRun(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, value, suffix));
    }
}
