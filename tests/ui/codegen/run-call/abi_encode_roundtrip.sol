//@ run-call: roundtrip 7 => 7

contract AbiEncodeRoundtrip {
    function roundtrip(uint256 value) external pure returns (uint256) {
        return abi.decode(abi.encode(value), (uint256));
    }
}
