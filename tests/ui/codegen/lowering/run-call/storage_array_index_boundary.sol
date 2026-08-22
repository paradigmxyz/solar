//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: read(uint256,uint256) 10, 9 => 0
//@[none, gas, size] run-call-fail: read(uint256,uint256) 10, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[none, gas, size] run-call-fail: read(uint256,uint256) 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@[none, gas, size] run-call: read(uint256,uint256) 256, 255 => 0
//@[none, gas, size] run-call-fail: read(uint256,uint256) 256, 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
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
