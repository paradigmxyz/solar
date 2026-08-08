//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: 0x354b9a2c => 0x0000000000000000000000000000000000000000000000000000000000000060

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
}
