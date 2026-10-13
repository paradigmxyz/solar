//@ run-call: lastByte => 0xa000000000000000000000000000000000000000000000000000000000000000
//@ run-call: dynamicOffset 0x5f => 0xa000000000000000000000000000000000000000000000000000000000000000
//@ run-call: dynamicSize 32 => 0xa000000000000000000000000000000000000000000000000000000000000000

// An entry point stores the initial free memory pointer, here 0xa0, only when
// it may observe the word at 0x40. Each read below starts at 0x5f, the last
// byte of that word, so the store must happen. A range whose offset or size
// is not a constant counts as touching the word.
contract FreeMemorySlotReads {
    function lastByte() external pure returns (bytes32) {
        assembly {
            return(0x5f, 0x20)
        }
    }

    function dynamicOffset(uint256 offset) external pure returns (bytes32) {
        assembly {
            return(offset, 0x20)
        }
    }

    function dynamicSize(uint256 size) external pure returns (bytes32) {
        assembly {
            return(0x5f, size)
        }
    }
}
