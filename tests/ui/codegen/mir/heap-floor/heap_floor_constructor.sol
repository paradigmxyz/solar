//@ codegen-matrix: standard
//@ run-call: count; constructor=[96, 7] => 7
//@ run-call: stored; constructor=[96, 7] => 0x46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21

// A constructor that resets the free memory pointer gets its own initial pointer back, recomputed
// from the copied argument blob. Without the floor, the allocation would zero the blob, and `m`
// is read back from it afterwards.
contract HeapFloorConstructor {
    bytes32 public stored;
    uint256 public count;

    constructor(uint256 n, uint256 m) {
        assembly { mstore(0x40, 0x80) }
        bytes memory b = new bytes(n);
        stored = keccak256(b);
        count = m;
    }
}
