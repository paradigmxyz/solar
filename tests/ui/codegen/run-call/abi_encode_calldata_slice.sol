//@ run-call: encPackedBytes(bytes,uint256,uint256) 0x414242, 1, 3 => 0x4242
//@ run-call: encPackedBytesReference(bytes,uint256,uint256) 0x414242, 1, 3 => 0x4242
//@ run-call: encBytes(bytes,uint256,uint256) 0x414242, 1, 3 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000024242000000000000000000000000000000000000000000000000000000000000
//@ run-call: encBytesReference(bytes,uint256,uint256) 0x414242, 1, 3 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000024242000000000000000000000000000000000000000000000000000000000000
//@ run-call: encPackedUint256(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x00000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encPackedUint256Reference(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x00000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encUint256(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encUint256Reference(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_calldata_slice.sol


contract AbiEncodeCalldataSlice {
    function encPackedBytes(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(data[start:end]);
    }

    function encPackedBytesReference(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(bytes(data[start:end]));
    }

    function encBytes(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(data[start:end]);
    }

    function encBytesReference(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(bytes(data[start:end]));
    }

    function encUint256(uint256[] calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(data[start:end]);
    }

    function encUint256Reference(uint256[] calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encode(data[start:end]);
    }

    function encPackedUint256(uint256[] calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(data[start:end]);
    }

    function encPackedUint256Reference(uint256[] calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(data[start:end]);
    }
}
