//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call-fail: test() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000031
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_empty_exception.sol

contract StorageArrayPopEmpty {
    uint256[] data;

    function test() external returns (bool) {
        data.pop();
        return true;
    }
}
