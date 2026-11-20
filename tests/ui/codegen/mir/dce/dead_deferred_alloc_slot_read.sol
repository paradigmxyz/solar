//@ compile-flags: -Ogas
//@ run-call: guarded => 64
//@ run-call: plain => 0
// A load of the free memory pointer's slot sees the bump of a deferred allocation that the backend
// places at run time, so `dce` keeps an unused one in a module that loads the slot. `guarded` reads
// back from a later object, which makes the backend place its allocations at run time.

contract DeadDeferredAllocSlotRead {
    struct S {
        uint256 x;
        uint256 y;
    }

    function guarded() external pure returns (uint256 d) {
        uint256 before;
        assembly ("memory-safe") {
            before := mload(0x40)
        }
        S memory dead = S(1, 2);
        uint256 after_;
        assembly ("memory-safe") {
            after_ := mload(0x40)
        }
        d = after_ - before;
        bytes memory buf = new bytes(32);
        assembly ("memory-safe") {
            if eq(mload(sub(buf, 0x20)), 12345) { revert(0, 0) }
        }
    }

    function plain() external pure returns (uint256 d) {
        uint256 before;
        assembly ("memory-safe") {
            before := mload(0x40)
        }
        S memory dead = S(1, 2);
        uint256 after_;
        assembly ("memory-safe") {
            after_ := mload(0x40)
        }
        d = after_ - before;
    }
}
