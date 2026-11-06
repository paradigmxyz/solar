//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: read 10, 9 => 0
//@ run-call-fail: read 10, 10 => Panic(0x32)
//@ run-call-fail: read 1, 1 => Panic(0x32)
//@ run-call: read 256, 255 => 0
//@ run-call-fail: read 256, 256 => Panic(0x32)
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
