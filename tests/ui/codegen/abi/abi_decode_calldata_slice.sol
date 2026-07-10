//@compile-flags: -Zcodegen --emit=mir

contract AbiDecodeCalldataSlice {
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
        //~^ ERROR: codegen does not support `abi.decode` from this calldata source yet
    }
}
