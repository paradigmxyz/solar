//@ compile-flags: -O none --emit=bin
// The constructor's assembly reads 2^64 - 384 bytes below a heap pointer, which the heap prefix
// reserves. The constructor's own memory fits below that prefix, but the frame of `wide`, which
// constructor code keeps on the heap, does not fit in what is left of the address range.
contract C {
    uint256 public stored;

    constructor(uint256 seed) {
        uint256 x = wide(seed);
        assembly {
            let p := mload(0x40)
            x := add(x, mload(sub(p, 0xfffffffffffffe80)))
        }
        stored = x;
    }

    function wide(uint256 seed) internal pure returns (uint256 total) { //~ ERROR: call frame with its heap prefix exceeds the addressable memory range
        uint256[40] memory words;
        for (uint256 i; i < 40; ++i) words[i] = seed + i;
        for (uint256 i; i < 40; ++i) total += words[i];
    }
}
