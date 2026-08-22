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
//@[none, gas, size] run-call: test() => 7
// ported-from: test/libsolidity/semanticTests/array/copying/copy_function_internal_storage_array.sol

contract InternalFunctionPointerDynamicStorageCopy {
    function() internal returns (uint256)[] x;
    function() internal returns (uint256)[] y;

    function test() external returns (uint256) {
        x = new function() internal returns (uint256)[](10);
        x[9] = a;
        y = x;
        return y[9]();
    }

    function a() public pure returns (uint256) {
        return 7;
    }
}
