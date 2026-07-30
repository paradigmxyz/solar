//@ compile-flags: -Zcodegen -Zdump=mir

// Unlike an array index, invalid calldata slice bounds revert with empty data.
// This covers both an end past the source length and a start after the end.
contract CalldataSliceBoundsRevert {
    function endPastLength(bytes calldata data, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return data[1:end];
    }

    function startAfterEnd(bytes calldata data, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return data[start:end];
    }
}
