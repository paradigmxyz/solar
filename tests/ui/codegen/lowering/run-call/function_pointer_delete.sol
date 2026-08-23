//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: callAfterSet => 7
//@ run-call-fail: callAfterDelete => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051
// ported-from: test/libsolidity/semanticTests/functionTypes/function_delete_storage.sol

contract FunctionPointerDelete {
    function() internal returns (uint256) target;

    function value() internal pure returns (uint256) {
        return 7;
    }

    function callAfterSet() external returns (uint256) {
        target = value;
        return target();
    }

    function callAfterDelete() external returns (uint256) {
        target = value;
        delete target;
        return target();
    }
}
