//@ codegen-matrix: standard
//@ run-call: direct 0 => true
//@ run-call: direct 1 => true
//@ run-call: viaHelper 0 => true
//@ run-call: viaHelper 1 => true

// A contract whose assembly is all marked memory-safe still raises a constant or calldata word
// stored into the free memory pointer slot to the heap floor. An external function's integer
// parameter is such a word, stored directly or passed to a helper that stores it, so the
// allocation after it lands above the floor and not at the address the argument names.
contract HeapFloorMemorySafeParams {
    function direct(uint256 p) external pure returns (bool above) {
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    function viaHelper(uint256 p) external pure returns (bool above) {
        setPointer(p);
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            above := gt(b, 0x7f)
        }
    }

    function setPointer(uint256 p) internal pure {
        assembly ("memory-safe") {
            mstore(0x40, p)
        }
    }
}
