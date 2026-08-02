//@ run-call: roundtrip 7 => 7
//@ run-call: hash 7 => 0xa66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688

contract AbiEncodeRoundtrip {
    function roundtrip(uint256 value) external pure returns (uint256) {
        return abi.decode(abi.encode(value), (uint256));
    }

    function hash(uint256 value) external pure returns (bytes32) {
        return keccak256(abi.encode(value));
    }
}
