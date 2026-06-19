//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract AbiEncodePackedCalldataSlice {
    function hash(bytes calldata data, uint256 start, uint256 end) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(data[start:end]));
        //~^ ERROR: codegen does not support packed encoding of calldata `bytes`/`string` yet
    }
}
