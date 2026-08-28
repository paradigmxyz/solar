//@ codegen-matrix: standard
//@ run-call: storeLong() => 0x0101010101010101010101010101010101010101010101010101010101010101, 0xff00000000000000000000000000000000000000000000000000000000000000, 33
//@ run-call: storeShort() => 0xffffffffff00000000000000000000000000000000000000000000000000000a, 5

// A memory `bytes` value is not guaranteed to have zeroed padding after its
// last byte. Copying it to storage must mask the final partial word, like
// solc, so stale padding never reaches storage.
contract StorageBytesDirtyTail {
    bytes internal stored;

    function storeLong() external returns (bytes32 first, bytes32 last, uint256 length) {
        bytes memory value;
        assembly {
            value := mload(0x40)
            mstore(value, 33)
            mstore(add(value, 0x20), 0x0101010101010101010101010101010101010101010101010101010101010101)
            mstore(add(value, 0x40), not(0))
            mstore(0x40, add(value, 0x60))
        }
        stored = value;
        bytes32 dataSlot;
        assembly {
            mstore(0, stored.slot)
            dataSlot := keccak256(0, 0x20)
            first := sload(dataSlot)
            last := sload(add(dataSlot, 1))
        }
        length = stored.length;
    }

    function storeShort() external returns (bytes32 slot, uint256 length) {
        bytes memory value;
        assembly {
            value := mload(0x40)
            mstore(value, 5)
            mstore(add(value, 0x20), not(0))
            mstore(0x40, add(value, 0x40))
        }
        stored = value;
        assembly {
            slot := sload(stored.slot)
        }
        length = stored.length;
    }
}
