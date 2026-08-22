//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: test => 1, 2, 3
//@[gas] run-call: test => 1, 2, 3
//@[size] run-call: test => 1, 2, 3
// ported-from: test/libsolidity/semanticTests/array/pop/array_pop_array_transition.sol

contract StorageNestedArrayPop {
    uint16[] inner = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    uint16[][] data;

    function test() external returns (uint256 x, uint256 y, uint256 z) {
        for (uint256 i = 1; i <= 48; ++i) data.push(inner);
        for (uint256 j = 1; j <= 10; ++j) data.pop();
        x = data[data.length - 1][0];
        for (uint256 k = 1; k <= 10; ++k) data.pop();
        y = data[data.length - 1][1];
        for (uint256 l = 1; l <= 10; ++l) data.pop();
        z = data[data.length - 1][2];
        for (uint256 m = 1; m <= 18; ++m) data.pop();
        delete inner;
    }
}
