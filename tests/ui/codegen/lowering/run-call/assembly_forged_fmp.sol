//@ compile-flags: -Ogas --allow=2018
//@ run-call: f(uint256) 0 => 0x0000000000000000000000000000000000000000000000000000000000001111
//@ run-call: f(uint256) 1 => 0x0000000000000000000000000000000000000000000000000000000011110000

// Assembly can store a low word in the free-memory-pointer slot, so the word read back from it
// is not a heap pointer and the return observes the scratch word below the heap.
contract AssemblyForgedFmp {
    function f(uint256 n) external pure returns (uint256) {
        assembly {
            mstore(0x00, 0x1111)
            switch and(n, 1)
            case 0 { mstore(0x40, n) }
            default { mstore(0x40, add(n, 1)) }
            let p := mload(0x40)
            return(p, 0x20)
        }
    }
}
