//@ run-call: encodeFixed [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeDynamic [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeCalldata [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeBytes [[0x01, 0x02], [0x03, 0x04]] => 0x0d55ebf6741e18b57f3691519f8e5f84c50c0987a6361cb4261a39f76c12a217

contract AbiPackedNestedArrays {
    function encodeFixed(uint256[2][2] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    function encodeDynamic(uint256[2][] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    function encodeCalldata(uint256[2][] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    function encodeBytes(bytes1[2][2] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }
}
