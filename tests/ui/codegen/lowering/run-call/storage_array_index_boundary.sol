//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: read(uint256,uint256) 10, 9 => 0
//@ run-call-fail: read(uint256,uint256) 10, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: read(uint256,uint256) 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: read(uint256,uint256) 256, 255 => 0
//@ run-call-fail: read(uint256,uint256) 256, 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// ported-from: test/libsolidity/semanticTests/array/array_storage_index_boundary_test.sol

contract StorageArrayIndexBoundary {
    uint256[] private values;

    function read(uint256 length, uint256 index) external returns (uint256) {
        while (values.length < length) {
            values.push();
        }
        while (values.length > length) {
            values.pop();
        }
        return values[index];
    }
}
