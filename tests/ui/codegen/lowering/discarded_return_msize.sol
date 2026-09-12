//@ compile-flags: -Osize
//@ run-call: expandedMemory => 65568
//@ run-call: conditionalMemory true => 4128
//@ run-call: conditionalMemory false => 96

contract DiscardedReturnMsize {
    function readFarMemory() internal pure returns (uint256 value) {
        assembly {
            value := mload(0x10000)
        }
    }

    function expandedMemory() external pure returns (uint256 size) {
        readFarMemory();
        assembly {
            size := msize()
        }
    }
    function conditionalMemory(bool touch) external pure returns (uint256 size) {
        assembly {
            if touch { pop(mload(4096)) }
            size := msize()
        }
    }
}
