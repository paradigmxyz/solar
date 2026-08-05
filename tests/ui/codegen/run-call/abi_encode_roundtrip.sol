//@ run-call: roundtrip 7 => 7
//@ run-call: bytesRoundtrip 0x010203 => 0x010203
//@ run-call: wordsRoundtrip [1, 2, 3] => 6
//@ run-call: nestedWordsRoundtrip [[1, 2], [3]] => 3
//@ run-call: mixedRoundtrip() => 9, 3, 3
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

    function mixedRoundtrip() external pure returns (uint256, uint256, uint8) {
        uint256[2] memory numbers;
        numbers[0] = 7;
        numbers[1] = 9;
        bytes memory tail = hex"010203";
        (uint256[2] memory decodedNumbers, bytes memory decodedTail, uint8 flag) =
            abi.decode(abi.encode(numbers, tail, uint8(3)), (uint256[2], bytes, uint8));
        return (decodedNumbers[1], decodedTail.length, flag);
    }

    function hash(uint256 value) external pure returns (bytes32) {
        return keccak256(abi.encode(value));
    }
}
