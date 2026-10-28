//@ compile-flags: -Ogas --emit=bin

// A runtime slot can be 0x40. The local lifetime check and callee summaries must treat its
// loaded value as a possible free memory pointer before scratch memory is reused.
contract Test {
    uint256 private savedPointer;

    function local(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(slot) }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function readPointer(uint256 slot) internal pure returns (uint256 result) {
        assembly { result := mload(slot) }
    }

    function forwardPointer(uint256 slot) internal pure returns (uint256) {
        return readPointer(slot);
    }

    function returnedThroughWrapper(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer = forwardPointer(slot);
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function stashMemory(uint256 slot) internal pure {
        assembly { mstore(0, mload(slot)) }
    }

    function forwardStash(uint256 slot) internal pure {
        stashMemory(slot);
    }

    function capturedThroughWrapper(uint256 slot) external pure returns (uint256 result) {
        forwardStash(slot); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mload(0), 0x20)) }
    }

    function stashStorage(uint256 slot) internal {
        assembly { sstore(savedPointer.slot, mload(slot)) }
    }

    function capturedInStorage(uint256 slot) external returns (uint256 result) {
        stashStorage(slot); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(sload(savedPointer.slot), 0x20)) }
    }

    /// @custom:solar-noinline
    function temporary() internal pure returns (bytes32 digest) {
        /// @custom:solar-scratch
        { digest = keccak256(abi.encode(uint256(101))); }
    }

    function callerKeeps(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(slot) }
        require(temporary() != bytes32(0));
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function forwardedLoad(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            let original := mload(slot)
            mstore(0x40, original)
            pointer := mload(0x40)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function readForwarded(uint256 slot) internal pure returns (uint256 result) {
        assembly {
            let original := mload(slot)
            mstore(0x40, original)
            result := mload(0x40)
        }
    }

    function returnedForwardedLoad(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer = readForwarded(slot);
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    // An unaligned load can recover the free pointer from the overlapping part of its word.
    function overlappingLoad() external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := shr(8, mload(65)) }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    // With slot = 0, the copy replaces the scalar with the free pointer. The earlier store
    // cannot justify treating the later load as a known scalar across the intervening copy.
    function copyInvalidatesStoredValue(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(slot, 0)
            mcopy(slot, 0x40, 0x20) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
            pointer := mload(slot)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    /// @custom:solar-noinline
    function copyPointer(uint256 slot) internal pure {
        assembly { mcopy(slot, 0x40, 0x20) }
    }

    function callInvalidatesStoredValue(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { mstore(slot, 0) }
        copyPointer(slot); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block
        assembly { pointer := mload(slot) }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    // A byte write also discards the full-word fact, even when its value is constant.
    function byteInvalidatesStoredValue(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(slot, 0)
            mstore8(add(slot, 31), 128)
            pointer := mload(slot)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }
}
