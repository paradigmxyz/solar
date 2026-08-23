//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test() => 7
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
