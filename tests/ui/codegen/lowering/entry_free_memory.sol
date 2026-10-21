//@ compile-flags: -Ogas
//@ run-call: 0xececca9f => 0x0000000000000000000000000000000000000000000000000000000000000080

contract EntryFreeMemory {
    function observedSize() external pure {
        assembly {
            mstore(0, msize()) //~ WARN: `msize` is not exact when optimizing
            return(0, 32)
        }
    }

    // Returning the reserved free-memory-pointer word observes its initialized value even though
    // the function contains no explicit FMP operation.
    function exposedPointer() external pure {
        assembly {
            return(0x40, 0x20)
        }
    }
}
