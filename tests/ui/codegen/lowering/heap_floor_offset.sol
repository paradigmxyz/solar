//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: HeapFloorOffsets::stored => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: HeapFloorOffsets::hash(uint256) 5 => 0xbfd358e93f18da3ed276c3afdbdba00b8f0b6008a03476a6a86bd6320ee6938b

// Memory laid out from the heap floor addresses its words as the floor plus a constant, in the
// runtime code and in the constructor, whose floor comes from the argument blob.
contract HeapFloorOffsets {
    bytes32 public stored;

    constructor() {
        stored = keccak256(abi.encode(uint256(1), uint256(2), uint256(3)));
    }

    function hash(uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encode(x, x + 1));
    }
}
