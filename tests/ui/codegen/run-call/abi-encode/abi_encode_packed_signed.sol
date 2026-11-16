//@ filecheck:
// CHECK: @module AbiEncodePackedSigned
//@ codegen-matrix: standard
//@ run-call: mutatedInput => 0x0109
//@ run-call: signedCall -1 => -1
//@ run-call: signedCall -128 => -128
//@ run-call: signedCall 127 => 127
//@ run-call: signedWord -1 => -1
//@ run-call: signedWord -128 => -128
//@ run-call: signedWord 127 => 127
//@ run-call: signedUserWord => -1
//@ run-call: signedUserMemory => -1
//@ run-call: signedWords => true
//@ run-call: signedError => true
//@ run-call: firstByteInt8 0, -1 => 0x00
//@ run-call: firstByteInt16 0, -1 => 0x00
//@ run-call: firstByteInt32 0, -1 => 0x00
//@ run-call: firstByteSigned16 => 0x00
//@ run-call: packedHash 0, -1, 0x000000 => 0xb6a3d3257d6e2bc9006a983edcb917248decccd41807b311f1d9317609a2bb05
//@ run-call: packedHashLocal 0, -1, 0x000000 => 0xb6a3d3257d6e2bc9006a983edcb917248decccd41807b311f1d9317609a2bb05
//@ run-call: packedDynamicHashLocal 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x123456 => 0xca93ec886a82f406d0a1cee7dcbe1930b1cee7695d89f3c0f2b849eab8593ee4
//@ run-call: packedDynamicHashModified 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x123456 => 0xca93ec886a82f406d0a1cee7dcbe1930b1cee7695d89f3c0f2b849eab8593ee4

type Signed16 is int16;

contract AbiEncodePackedSigned {
    error SignedError(int8 value);

    function failSigned() external pure {
        revert SignedError(-1);
    }

    function signedError() external returns (bool) {
        (bool success, bytes memory data) = address(this).call(
            abi.encodeWithSelector(this.failSigned.selector)
        );
        return !success && keccak256(data) == keccak256(
            abi.encodeWithSelector(SignedError.selector, int256(-1))
        );
    }

    function identity(int8 value) external pure returns (int8) {
        return value;
    }

    function signedCall(int8 value) external view returns (int8) {
        return this.identity(value);
    }

    function signedWord(int8 value) external pure returns (int256 result) {
        bytes memory encoded = abi.encode(value);
        assembly { result := mload(add(encoded, 32)) }
    }

    function signedUserWord() external pure returns (int256 result) {
        bytes memory encoded = abi.encode(Signed16.wrap(-1));
        assembly { result := mload(add(encoded, 32)) }
    }

    function signedUserMemory() external pure returns (int256 result) {
        Signed16[] memory values = new Signed16[](1);
        values[0] = Signed16.wrap(-1);
        assembly { result := mload(add(values, 32)) }
    }

    function signedWords() external pure returns (bool) {
        bytes memory encoded = abi.encode(
            int8(-1),
            int16(-1),
            int24(-1),
            int32(-1),
            int40(-1),
            int48(-1),
            int56(-1),
            int64(-1),
            int72(-1),
            int80(-1),
            int88(-1),
            int96(-1),
            int104(-1),
            int112(-1),
            int120(-1),
            int128(-1),
            int136(-1),
            int144(-1),
            int152(-1),
            int160(-1),
            int168(-1),
            int176(-1),
            int184(-1),
            int192(-1),
            int200(-1),
            int208(-1),
            int216(-1),
            int224(-1),
            int232(-1),
            int240(-1),
            int248(-1),
            int256(-1)
        );
        for (uint i = 0; i < encoded.length; ++i) {
            if (encoded[i] != 0xff) return false;
        }
        return true;
    }

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

    function shorten(bytes memory value) private pure returns (uint8) {
        assembly { mstore(value, 1) }
        return 9;
    }

    function mutatedInput() external pure returns (bytes memory) {
        bytes memory value = hex"010203";
        return abi.encodePacked(value, shorten(value));
    }

}
