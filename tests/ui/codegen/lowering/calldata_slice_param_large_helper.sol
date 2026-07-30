//@compile-flags: -Zcodegen

contract CalldataSliceParamLargeHelper {
    function lastWord(bytes calldata data) external pure returns (uint256) {
        return _last(data);
    }

    // This body deliberately exceeds the ordinary lowering-time inline budget.
    // It must still inline because internal frames cannot carry a calldata slice.
    function _last(bytes calldata data) internal pure returns (uint256 x) {
        assembly {
            x := calldataload(add(data.offset, sub(data.length, 0x20)))
        }
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
        x |= 0;
    }
}
