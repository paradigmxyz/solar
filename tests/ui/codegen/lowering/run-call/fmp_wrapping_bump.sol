//@ compile-flags: -Ogas --allow=2018
//@ run-call: bump(uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc0 => 64
//@ run-call: bump(uint256) 32 => 0

// An assembly bump that wraps around to the free-memory-pointer word is not the allocator's
// bump, so the store of the sum is not dead before the exit that reads through it.
contract FmpWrappingBump {
    function bump(uint256 n) external returns (uint256 result) {
        assembly {
            let q := add(mload(0x40), n)
            mstore(0x40, q)
            return(q, 0x20)
        }
    }
}
