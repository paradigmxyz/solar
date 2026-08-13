//@ compile-flags: -Osize
//@ run-call: expandedMemory() => 65568

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
}
