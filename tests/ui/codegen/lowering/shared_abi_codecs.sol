//@ revisions: ir gas size
//@[ir] compile-flags: -Osize -Zdump=mir
//@[ir] filecheck:
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[gas] run-call: roundTrip 3, [[5], [8]] => 16
//@[size] run-call: roundTrip 3, [[5], [8]] => 16

// CHECK-LABEL: fn @encodeFirst
// CHECK: internal_call @__abi_decode_calldata_[[DECODER:[0-9]+]]
// CHECK: internal_call @__abi_encode_[[ENCODER:[0-9]+]]
// CHECK: fn @__abi_decode_calldata_[[DECODER]]
// CHECK: ret
// CHECK-LABEL: fn @encodeSecond
// CHECK: internal_call @__abi_decode_calldata_[[DECODER]]
// CHECK: internal_call @__abi_encode_[[ENCODER]]
// CHECK: fn @__abi_encode_[[ENCODER]]
// CHECK: mstore
// CHECK: ret

contract SharedAbiCodecs {
    struct Payload {
        uint256 tag;
        uint256[][] values;
    }

    function encodeFirst(Payload memory payload) public pure returns (bytes memory) {
        return abi.encode(payload);
    }

    function encodeSecond(Payload memory payload) public pure returns (bytes memory) {
        return abi.encode(payload);
    }

    function roundTrip(uint256 tag, uint256[][] calldata values)
        external
        pure
        returns (uint256)
    {
        Payload memory payload = Payload({tag: tag, values: values});
        Payload memory decoded = abi.decode(encodeFirst(payload), (Payload));
        return decoded.tag + decoded.values[0][0] + decoded.values[1][0];
    }
}
