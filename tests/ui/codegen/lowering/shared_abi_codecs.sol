//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zdump=mir
//@[ir] filecheck:
//@ run-call: roundTrip 3, [[5], [8]] => 16

// CHECK-LABEL: fn @encodeFirst
// CHECK: internal_call @[[DECODER:decode_calldata_type]]
// CHECK: internal_call @[[ENCODER:encodeFirst.body]]
// CHECK: ret
// CHECK-LABEL: fn @encodeSecond
// CHECK: internal_call @[[DECODER]]
// CHECK: internal_call @[[ENCODER]]
// CHECK: fn @[[DECODER]]
// CHECK: fn @[[ENCODER]]
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
