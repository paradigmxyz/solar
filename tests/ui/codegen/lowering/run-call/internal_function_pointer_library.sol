//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f(uint256[]) [1, 7, 3] => 11
// ported-from: test/libsolidity/semanticTests/functionTypes/function_type_library_internal.sol

library InternalFunctionPointerLibraryUtils {
    function reduce(
        uint256[] memory array,
        function(uint256, uint256) internal returns (uint256) callback,
        uint256 init
    ) internal returns (uint256) {
        for (uint256 i = 0; i < array.length; ++i) {
            init = callback(array[i], init);
        }
        return init;
    }

    function sum(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}

contract InternalFunctionPointerLibrary {
    function f(uint256[] memory values) public returns (uint256) {
        return InternalFunctionPointerLibraryUtils.reduce(values, InternalFunctionPointerLibraryUtils.sum, 0);
    }
}
