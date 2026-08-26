//@ filecheck:
// CHECK: @module AbiEncodePackedSigned
//@ codegen-matrix: standard
//@ run-call: firstByteInt8(uint8,int8) 0, -1 => 0x00
//@ run-call: firstByteInt16(uint8,int16) 0, -1 => 0x00
//@ run-call: firstByteInt32(uint8,int32) 0, -1 => 0x00
//@ run-call: firstByteSigned16() => 0x00
//@ run-call: packedHash(uint8,int16,bytes3) 0, -1, 0x000000 => 0xb6a3d3257d6e2bc9006a983edcb917248decccd41807b311f1d9317609a2bb05
//@ run-call: packedHashLocal(uint8,int16,bytes3) 0, -1, 0x000000 => 0xb6a3d3257d6e2bc9006a983edcb917248decccd41807b311f1d9317609a2bb05
//@ run-call: packedDynamicHashLocal(bytes32,bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x123456 => 0xca93ec886a82f406d0a1cee7dcbe1930b1cee7695d89f3c0f2b849eab8593ee4
//@ run-call: packedDynamicHashModified(bytes32,bytes) 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x123456 => 0xca93ec886a82f406d0a1cee7dcbe1930b1cee7695d89f3c0f2b849eab8593ee4

type Signed16 is int16;

contract AbiEncodePackedSigned {
    modifier packedWasAllocated() {
        _;
        uint256 pointer;
        assembly {
            pointer := mload(0x40)
        }
        require(pointer > 0x80);
    }

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

    function firstByteSigned16() external pure returns (bytes1) {
        bytes memory encoded = abi.encodePacked(uint8(0), Signed16.wrap(int16(-1)));
        return encoded[0];
    }

    function packedHash(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(prefix, value, suffix));
    }

    function packedHashLocal(uint8 prefix, int16 value, bytes3 suffix)
        external
        pure
        returns (bytes32)
    {
        bytes memory encoded = abi.encodePacked(prefix, value, suffix);
        return keccak256(encoded);
    }

    function packedDynamicHashLocal(bytes32 prefix, bytes calldata value)
        external
        pure
        returns (bytes32)
    {
        bytes memory encoded = abi.encodePacked(prefix, value);
        return keccak256(encoded);
    }

    function packedDynamicHashModified(bytes32 prefix, bytes calldata value)
        external
        pure
        packedWasAllocated
        returns (bytes32)
    {
        bytes memory encoded = abi.encodePacked(prefix, value);
        return keccak256(encoded);
    }
}
