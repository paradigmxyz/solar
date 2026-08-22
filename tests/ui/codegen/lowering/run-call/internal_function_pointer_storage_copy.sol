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
//@[none, gas, size] run-call: one() => 3
//@[none, gas, size] run-call-fail: two() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051
// ported-from: test/libsolidity/semanticTests/array/copying/copy_internal_function_array_to_storage.sol

contract InternalFunctionPointerStorageCopy {
    function() internal returns (uint256)[20] values;
    int256 mutex;

    function one() public returns (uint256) {
        function() internal returns (uint256)[20] memory memory_values;
        values = memory_values;
        return 3;
    }

    function two() public returns (uint256) {
        if (mutex > 0) return 7;
        mutex = 1;
        values[0]();
        return 2;
    }
}
