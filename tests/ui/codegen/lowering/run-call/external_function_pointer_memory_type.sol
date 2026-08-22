//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: main() => true
// ported-from: test/libsolidity/semanticTests/functionTypes/external_functions_with_calldata_args_assigned_to_function_pointers_with_memory_type.sol

contract ExternalFunctionPointerMemoryType {
    function g(string calldata) external pure returns (bool) {
        return true;
    }

    function main() external returns (bool) {
        function(string memory) external returns (bool) ptr = this.g;
        return ptr("testString");
    }
}
