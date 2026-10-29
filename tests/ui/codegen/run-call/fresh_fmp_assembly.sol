//@ codegen-matrix: standard
//@ run-call: beforeAlloc => 0x80, 0x80
//@ run-call: afterAlloc => 0xc0
//@ run-call: skipped => 0xc0, 0x01
//@ run-call: scratch => 0x80, 0x80, 0xc0

// Memory-safe assembly that reads or moves the free memory pointer sees the word solc would
// leave there, whether or not the compiler's own first allocation reads the heap floor.
contract Test {
    // The raw read comes first and keeps the entry's initialization; the allocation after it
    // starts at the same address.
    function beforeAlloc() external pure returns (uint256 slot, uint256 ptr) {
        assembly ("memory-safe") {
            slot := mload(0x40)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            ptr := b
        }
    }

    // The allocation bumps the slot that assembly reads next.
    function afterAlloc() external pure returns (uint256 slot) {
        bytes memory b = new bytes(32);
        b[0] = 0x01;
        assembly ("memory-safe") {
            slot := mload(0x40)
        }
    }

    // Assembly reserves a word, so the allocation after it follows the moved pointer.
    function skipped() external pure returns (uint256 ptr, uint256 first) {
        assembly ("memory-safe") {
            mstore(0x40, add(mload(0x40), 0x40))
        }
        bytes memory b = new bytes(32);
        b[0] = 0x01;
        assembly ("memory-safe") {
            ptr := b
            first := shr(248, mload(add(b, 0x20)))
        }
    }

    // Scratch use of the pointer, such as hashing at it, leaves the slot as it was.
    function scratch() external pure returns (uint256 start, uint256 mid, uint256 end) {
        assembly ("memory-safe") {
            start := mload(0x40)
            mstore(start, 1)
            pop(keccak256(start, 0x20))
            mid := mload(0x40)
        }
        bytes memory b = new bytes(32);
        assembly ("memory-safe") {
            end := mload(0x40)
        }
    }
}
