//@ codegen-matrix: standard
//@ run-call: one => 1
//@ run-call: pointer => 0x0000000000000000000000000000000000000000000000000000000000000080
//@ run-call: span => 1, 2, 128

// The dispatcher never initializes the free-memory pointer, so a route that returns the slot
// without overwriting it keeps its wrapper, which does. The wide scalar returns overwrite the
// slot before they return it, so they may join the dispatcher.
contract DispatchReturnFreeMemorySlot {
    function one() external pure returns (uint256) {
        return 1;
    }

    function pointer() external pure returns (uint256) {
        assembly {
            return(0x40, 0x20)
        }
    }

    function span() external pure returns (uint256, uint256, uint256) {
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            return(0, 0x60)
        }
    }
}
