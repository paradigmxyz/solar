//@ run-call: read(uint256[3],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: read(uint256[3],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readDynamic(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamic(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readBytes(bytes) 0x010203 => 0x02
//@ run-call: readPair((uint8,uint8)) (7, 9) => 7
//@ run-call: readDynamicCalldata(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamicCalldata(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readBytesCalldata(bytes) 0x010203 => 0x02
//@ run-call: readDynamicPair((uint256,bytes)) (7, 0x010203) => 10
//@ run-call: readMixed(uint8,bytes) 1, 0x010203 => 4

contract AbiFixedArray {
    struct Pair {
        uint8 first;
        uint8 second;
    }

    struct DynamicPair {
        uint256 value;
        bytes data;
    }

    enum Mode {
        Zero,
        One
    }

    function read(uint256[3] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamic(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamicCalldata(uint256[] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readBytes(bytes memory values) external pure returns (bytes1) {
        return values[1];
    }

    function readBytesCalldata(bytes calldata values) external pure returns (bytes1) {
        return values[1];
    }

    function readPair(Pair memory value) external pure returns (uint256) {
        return value.first;
    }

    function readDynamicPair(DynamicPair memory value) external pure returns (uint256) {
        return value.value + value.data.length;
    }

    function readMixed(Mode mode, bytes memory data) external pure returns (uint256) {
        return uint256(mode) + data.length;
    }
}
