//@run-call: decode 0xdeadbeef000000000000000000000000000000000000000000000000000000000000002a => 42

contract AbiDecodeCalldataSlice {
    function decode(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data[4:], (uint256));
    }
}
