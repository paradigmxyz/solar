//@ run-call: read(uint256[3],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: read(uint256[3],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readDynamic(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamic(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readBytes(bytes) 0x010203 => 0x02
//@ run-call: readPair((uint8,uint8)) (7, 9) => 7

contract AbiFixedArray {
    struct Pair {
        uint8 first;
        uint8 second;
    }

    function read(uint256[3] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamic(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readBytes(bytes memory values) external pure returns (bytes1) {
        return values[1];
    }

    function readPair(Pair memory value) external pure returns (uint256) {
        return value.first;
    }
}
