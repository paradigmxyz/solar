//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Packed encoding writes each value's top `size` bytes: fixed-bytes values
// are already left-aligned and must not be shifted again. Hashes use an
// unbumped scratch buffer; materialized encodings still use bytes objects.
contract AbiEncodePackedMixed {
    // CHECK-LABEL: fn @fixedBytesArg{{[( ]}}
    // CHECK: keccak256_packed (u256 arg0, u160 {{v[0-9]+}}, bytes2 {{v[0-9]+}})
    function fixedBytesArg(uint a, address b, bytes2 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    // CHECK-LABEL: fn @dynamicArg{{[( ]}}
    // CHECK: keccak256_packed (bytes32 arg0, bytes arg1)
    function dynamicArg(bytes32 h, bytes memory tail) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(h, tail));
    }

    // CHECK-LABEL: fn @materialized{{[( ]}}
    // CHECK: abi_encode_packed (u16 {{v[0-9]+}}, bytes arg1, u8 {{v[0-9]+}})
    function materialized(uint16 a, bytes memory mid, bool b) external pure returns (bytes memory) {
        return abi.encodePacked(a, mid, b);
    }

    // CHECK-LABEL: fn @hashArray{{[( ]}}
    // CHECK: [[BYTES:v[0-9]+]] = abi_encode_packed (array<word> memoryarray<1> arg0)
    // CHECK: keccak256_bytes [[BYTES]]
    function hashArray(bytes32[] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    // Signed packed values are sign-extended words. Coalescing them with a
    // preceding field must mask the sign extension before shifting, or the
    // high bits overwrite that field.
    // CHECK-LABEL: fn @signedStaticRun{{[( ]}}
    // CHECK: keccak256_packed (u8 {{v[0-9]+}}, i16 {{v[0-9]+}}, bytes3 {{v[0-9]+}})
    function signedStaticRun(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, value, suffix));
    }
}
