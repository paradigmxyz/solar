//@ run-call: inspect [0x0102, 0x040506] => 2, 2, 1, 2, 3, 4, 5, 6
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_indexing_dynamic_bytes_v2.sol

pragma abicoder v2;

contract AbiCalldataDynamicBytesArray {
    function inspect(bytes[] calldata values)
        external
        pure
        returns (
            uint256 count,
            uint256 firstLength,
            uint256 firstByte,
            uint256 secondByte,
            uint256 secondLength,
            uint256 thirdByte,
            uint256 fourthByte,
            uint256 fifthByte
        )
    {
        count = values.length;
        firstLength = values[0].length;
        firstByte = uint8(values[0][0]);
        secondByte = uint8(values[0][1]);
        secondLength = values[1].length;
        thirdByte = uint8(values[1][0]);
        fourthByte = uint8(values[1][1]);
        fifthByte = uint8(values[1][2]);
    }
}
