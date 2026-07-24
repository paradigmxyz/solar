//@ check-pass
//@compile-flags: -Zcodegen --emit=bin-runtime

contract CalldataSliceReturn {
    function whole(bytes calldata data) external pure returns (bytes calldata) {
        return data;
    }

    function tail(bytes calldata data) external pure returns (bytes calldata) {
        return data[4:];
    }

    function words(uint256[] calldata values)
        external
        pure
        returns (uint256[] calldata)
    {
        return values;
    }
}
