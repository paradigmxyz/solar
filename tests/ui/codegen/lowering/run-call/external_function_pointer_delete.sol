//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: clear() => true
// ported-from: test/libsolidity/semanticTests/functionTypes/function_external_delete_storage.sol

contract ExternalFunctionPointerDelete {
    function() external target;

    function value() external {}

    function clear() external returns (bool) {
        target = this.value;
        delete target;
        function() external zero;
        return target == zero;
    }
}
