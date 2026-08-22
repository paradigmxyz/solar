//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call-fail: test() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000031
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_empty_exception.sol

contract StorageArrayPopEmpty {
    uint256[] data;

    function test() external returns (bool) {
        data.pop();
        return true;
    }
}
