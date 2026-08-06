//@ run-call: firstByteInt8(uint8,int8) 0, -1 => 0x00
//@ run-call: firstByteInt16(uint8,int16) 0, -1 => 0x00
//@ run-call: firstByteInt32(uint8,int32) 0, -1 => 0x00
//@ run-call: packedHash(uint8,int16,bytes3) 0, -1, 0x000000 => 0xb6a3d3257d6e2bc9006a983edcb917248decccd41807b311f1d9317609a2bb05

contract AbiEncodePackedSigned {
    function firstByteInt8(uint8 prefix, int8 value) external pure returns (bytes1) {
        bytes memory encoded = abi.encodePacked(prefix, value);
        return encoded[0];
    }

    function firstByteInt16(uint8 prefix, int16 value) external pure returns (bytes1) {
        bytes memory encoded = abi.encodePacked(prefix, value);
        return encoded[0];
    }

    function firstByteInt32(uint8 prefix, int32 value) external pure returns (bytes1) {
        bytes memory encoded = abi.encodePacked(prefix, value);
        return encoded[0];
    }

    function packedHash(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, value, suffix));
    }
}
