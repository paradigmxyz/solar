//@ run-call: read(uint256[3],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: read(uint256[3],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readDynamic(uint256[],uint256) [1, 2, 3], 1 => 2
//@ run-call-fail: readDynamic(uint256[],uint256) [1, 2, 3], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract AbiFixedArray {
    function read(uint256[3] calldata values, uint256 index) external pure returns (uint256) {
        return values[index];
    }

    function readDynamic(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }
}
