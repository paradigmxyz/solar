//@ run-call: encPackedBytes(bytes,uint256,uint256) 0x414242, 1, 3 => 0x4242
//@ run-call: encPackedBytesReference(bytes,uint256,uint256) 0x414242, 1, 3 => 0x4242
//@ run-call: encBytes(bytes,uint256,uint256) 0x414242, 1, 3 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000024242000000000000000000000000000000000000000000000000000000000000
//@ run-call: encBytesReference(bytes,uint256,uint256) 0x414242, 1, 3 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000024242000000000000000000000000000000000000000000000000000000000000
//@ run-call: encPackedUint256(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x00000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encPackedUint256Reference(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x00000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encPackedUint8(uint8[],uint256,uint256) [1, 2, 3], 1, 3 => 0x00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003
//@ run-call: encUint256(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: encUint256Reference(uint256[],uint256,uint256) [65, 66, 66], 1, 3 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000420000000000000000000000000000000000000000000000000000000000000042
//@ run-call: testBytes()
//@ run-call: testUint256()
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

    function encPackedUint8(uint8[] calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(data[start:end]);
    }

    function compare(bytes memory x, bytes memory y) internal pure {
        assert(x.length == y.length);
        for (uint256 i = 0; i < x.length; ++i) {
            assert(x[i] == y[i]);
        }
    }

    function testBytes() external view {
        bytes memory test = new bytes(3);
        test[0] = 0x41;
        test[1] = 0x42;
        test[2] = 0x42;
        for (uint256 i = 0; i < test.length; ++i) {
            for (uint256 j = i; j <= test.length; ++j) {
                compare(this.encPackedBytes(test, i, j), this.encPackedBytesReference(test, i, j));
                compare(this.encBytes(test, i, j), this.encBytesReference(test, i, j));
            }
        }
    }

    function testUint256() external view {
        uint256[] memory test = new uint256[](3);
        test[0] = 0x41;
        test[1] = 0x42;
        test[2] = 0x42;
        for (uint256 i = 0; i < test.length; ++i) {
            for (uint256 j = i; j <= test.length; ++j) {
                compare(
                    this.encPackedUint256(test, i, j),
                    this.encPackedUint256Reference(test, i, j)
                );
                compare(this.encUint256(test, i, j), this.encUint256Reference(test, i, j));
            }
        }
    }
}
