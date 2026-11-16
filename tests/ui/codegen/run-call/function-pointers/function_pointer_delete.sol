//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: callAfterSet => 7
//@ run-call-fail: callAfterDelete => Panic(0x51)
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
