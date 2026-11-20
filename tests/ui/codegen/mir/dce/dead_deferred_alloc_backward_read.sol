//@ compile-flags: -Ogas
//@ run-call: back => 128
// An unused memory object leaves no allocation, so the next object starts at the heap start
// (`solc` starts it 64 bytes higher) and a read back from it lands below the heap.

contract DeadDeferredAllocBackwardRead {
    struct S {
        uint256 x;
        uint256 y;
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
