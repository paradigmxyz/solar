//@ compile-flags: -Ogas
//@ run-call: guarded => 64
//@ run-call: plain => 0
//@ run-call: back => 192
// Assembly can see where the backend places memory objects, so `dce` keeps an unused deferred
// allocation in a module with assembly. `guarded` reads back from a later object, which makes the
// backend place its allocations at run time, and its pointer loads see each bump. `back` reads
// below an object into the unused one and sees the spacing `solc` gives.

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

    function back() external pure returns (uint256 e) {
        S memory dead = S(1, 2);
        bytes memory buf = new bytes(32);
        assembly {
            e := mload(sub(buf, 0x20))
            e := add(buf, mul(e, 0))
        }
    }
}
