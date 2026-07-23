//@compile-flags: -Zcodegen -Zdump=mir

contract AbiDecodeCalldataSlice {
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
