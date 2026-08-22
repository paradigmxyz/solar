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
