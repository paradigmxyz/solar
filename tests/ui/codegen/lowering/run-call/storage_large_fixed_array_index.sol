//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test() => 7, 8
//@ run-call: hasExpectedLength() => true
//@ run-call: read(uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call-fail: read(uint256) 0x100000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract StorageLargeFixedArrayIndex {
    mapping(uint256 => uint256[2][2 ** 200]) private arrays;

    function test() external returns (uint256, uint256) {
        uint256[2][2 ** 200] storage array = arrays[0];
        uint256 index = 1 << 199;
        array[index][1] = 7;
        array[index + 1][0] = 8;
        return (array[index][1], array[index + 1][0]);
    }

    function read(uint256 index) external view returns (uint256) {
        return arrays[0][index][0];
    }

    function hasExpectedLength() external view returns (bool) {
        return arrays[0].length == 2 ** 200;
    }
}
