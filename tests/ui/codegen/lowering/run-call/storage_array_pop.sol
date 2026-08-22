//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test => 1, 0
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop.sol

contract StorageArrayPop {
    uint256[] data;

    function test() external returns (uint256 x, uint256 l) {
        data.push(7);
        data.push(3);
        x = data.length;
        data.pop();
        x = data.length;
        data.pop();
        l = data.length;
    }
}
