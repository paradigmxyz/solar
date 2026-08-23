//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
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
