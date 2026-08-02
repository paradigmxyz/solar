//@ run-call: roundtrip 7 => 7
//@ run-call: bytesRoundtrip 0x010203 => 0x010203
//@ run-call: wordsRoundtrip [1, 2, 3] => 6
//@ run-call: nestedWordsRoundtrip [[1, 2], [3]] => 3
//@ run-call: hash 7 => 0xa66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688

contract AbiEncodeRoundtrip {
    function roundtrip(uint256 value) external pure returns (uint256) {
        return abi.decode(abi.encode(value), (uint256));
    }

    function bytesRoundtrip(bytes memory value) external pure returns (bytes memory) {
        return abi.decode(abi.encode(value), (bytes));
    }

    function wordsRoundtrip(uint256[] memory value) external pure returns (uint256) {
        uint256[] memory decoded = abi.decode(abi.encode(value), (uint256[]));
        return decoded[0] + decoded[1] + decoded[2];
    }

    function nestedWordsRoundtrip(uint256[][] memory value) external pure returns (uint256) {
        uint256[][] memory decoded = abi.decode(abi.encode(value), (uint256[][]));
        return decoded[1][0];
    }

    function hash(uint256 value) external pure returns (bytes32) {
        return keccak256(abi.encode(value));
    }
}
