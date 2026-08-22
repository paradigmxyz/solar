//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test => 38, 28, 18
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_uint16_transition.sol

contract StorageArrayPopPackedTransition {
    uint16[] data;

    function test() external returns (uint16 x, uint16 y, uint16 z) {
        for (uint256 i = 1; i <= 48; ++i) data.push(uint16(i));
        for (uint256 j = 1; j <= 10; ++j) data.pop();
        x = data[data.length - 1];
        for (uint256 k = 1; k <= 10; ++k) data.pop();
        y = data[data.length - 1];
        for (uint256 l = 1; l <= 10; ++l) data.pop();
        z = data[data.length - 1];
        for (uint256 m = 1; m <= 18; ++m) data.pop();
    }
}
