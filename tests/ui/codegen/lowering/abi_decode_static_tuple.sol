//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract AbiDecodeStaticTuple {
    function decode(bytes memory data) external pure returns (uint256 a, bool b, address c) {
        return abi.decode(data, (uint256, bool, address));
    }
}
