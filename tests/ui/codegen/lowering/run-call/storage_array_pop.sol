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
//@[none] run-call: test => 1, 0
//@[gas] run-call: test => 1, 0
//@[size] run-call: test => 1, 0
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
