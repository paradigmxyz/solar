//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Packed encoding writes each value's top `size` bytes: fixed-bytes values
// are already left-aligned and must not be shifted again. Hashes use an
// unbumped scratch buffer; materialized encodings still use bytes objects.
contract AbiEncodePackedMixed {
    // CHECK-LABEL: fn @fixedBytesArg{{[( ]}}
    // CHECK: keccak256_packed (uint256 arg0, uint160 arg1, bytes2 {{v[0-9]+}})
    function fixedBytesArg(uint a, address b, bytes2 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    // CHECK-LABEL: fn @dynamicArg{{[( ]}}
    // CHECK: keccak256_packed (bytes32 arg0, bytes arg1)
    function dynamicArg(bytes32 h, bytes memory tail) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(h, tail));
    }

    // CHECK-LABEL: fn @materialized{{[( ]}}
    // CHECK: abi_encode_packed (uint16 arg0, bytes arg1, uint8 {{v[0-9]+}})
    function materialized(uint16 a, bytes memory mid, bool b) external pure returns (bytes memory) {
        return abi.encodePacked(a, mid, b);
    }

    // A word array's packed encoding is its elements as they lie in memory, so
    // the hash reads them in place.
    // CHECK-LABEL: fn @hashArray{{[( ]}}
    // CHECK: [[DATA:v[0-9]+]] = ptradd arg0, 32
    // CHECK: [[LEN:v[0-9]+]] = mload arg0
    // CHECK: [[SIZE:v[0-9]+]] = shl [[LEN]], 5
    // CHECK: keccak256 [[DATA]], [[SIZE]]
    function hashArray(bytes32[] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    // Signed packed values are sign-extended words. Coalescing them with a
    // preceding field must mask the sign extension before shifting, or the
    // high bits overwrite that field.
    // CHECK-LABEL: fn @signedStaticRun{{[( ]}}
    // CHECK: keccak256_packed (uint8 arg0, int16 arg1, bytes3 {{v[0-9]+}})
    function signedStaticRun(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, value, suffix));
    }
}
