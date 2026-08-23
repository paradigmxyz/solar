//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: encodeFixed [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeDynamic [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeCalldata [[1, 2], [3, 4]] => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d
//@ run-call: encodeBytes [[0x01, 0x02], [0x03, 0x04]] => 0x0d55ebf6741e18b57f3691519f8e5f84c50c0987a6361cb4261a39f76c12a217
//@ run-call: encodeFixedIndex [[1, 2], [3, 4]] => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0
//@ run-call: encodeDynamicIndex [[1, 2], [3, 4]] => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0
//@ run-call: encodeStructField((uint256[2])) ([1, 2]) => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0
//@ run-call: encodeCalldataIndex [[1, 2], [3, 4]] => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0

contract AbiPackedNestedArrays {
    struct Pair {
        uint256[2] row;
    }

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

    function encodeFixedIndex(uint256[2][2] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values[0]));
    }

    function encodeDynamicIndex(uint256[2][] memory values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values[0]));
    }

    function encodeStructField(Pair memory value) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(value.row));
    }

    function encodeCalldataIndex(uint256[2][] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values[0]));
    }
}
