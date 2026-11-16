//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: run() => 7

contract InternalFunctionPointerAssemblySlotStore {
    function() internal pure returns (uint256) pointer;

    function run() external returns (uint256) {
        function() internal pure returns (uint256) raw = seven;
        assembly {
            sstore(pointer.slot, raw)
        }
        return pointer();
    }

    function seven() internal pure returns (uint256) {
        return 7;
    }
}
