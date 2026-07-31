//@compile-flags: -Zcodegen -Zdump=mir

contract AbiHeadSizeOverflow {
    function oversized(
        uint256[18446744073709551616] calldata values //~ ERROR: type too large for calldata
    ) external pure returns (uint256) {
        return values[0];
    }

    function oversizedDynamicReturn()
        external
        pure
        returns (uint256[][18446744073709551616] memory values) //~ ERROR: type too large for memory
    {}

    function oversizedInRangeDynamicReturn()
        external
        pure
        returns (uint256[][18446744073709551615] memory values) //~ ERROR: type too large for memory
    {}
}
