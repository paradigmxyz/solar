//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: 0x354b9a2c => 0x0000000000000000000000000000000000000000000000000000000000000060
//@ run-call: 0xececca9f => 0x0000000000000000000000000000000000000000000000000000000000000080

contract EntryFreeMemory {
    // `msize` observes the memory expansion caused by initializing Solidity's free-memory pointer,
    // even when the entry never allocates or loads the pointer.
    // CHECK: push 128
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mstore
    // CHECK: msize
    function observedSize() external pure {
        assembly {
            mstore(0, msize())
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
