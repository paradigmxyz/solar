//@ compile-flags: -Ogas --allow=2018
//@ run-call-fail: f() => 0x0000000000000000000000000000000000000000000000000000000000001234

// Assembly forges the length of a memory array, so a checked index wraps around to scratch
// memory. The read observes the constant store there, which memory liveness must keep.
contract WrappedMemoryIndex {
    function f() external pure {
        uint256[] memory a = new uint256[](1);
        uint256 i;
        assembly {
            mstore(a, not(0))
            mstore(0x00, 0x1234)
            i := shr(5, sub(0, add(a, 0x20)))
        }
        uint256 v = a[i];
        assembly {
            mstore(0x20, v)
            revert(0x20, 0x20)
        }
    }
}
