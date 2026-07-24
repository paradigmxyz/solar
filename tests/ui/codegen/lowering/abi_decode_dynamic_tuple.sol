//@compile-flags: -Zcodegen -Zdump=mir

contract AbiDecodeDynamicTuple {
    function decode(bytes memory data)
        external
        pure
        returns (uint256 a, string memory s, bytes memory b)
    {
        return abi.decode(data, (uint256, string, bytes));
    }

    function roundtrip(uint256 a, string memory s, bytes memory b)
        external
        pure
        returns (uint256, string memory, bytes memory)
    {
        return abi.decode(abi.encode(a, s, b), (uint256, string, bytes));
    }

    function decodeBytes(bytes memory data) external pure returns (bytes memory) {
        return abi.decode(data, (bytes));
    }
}
