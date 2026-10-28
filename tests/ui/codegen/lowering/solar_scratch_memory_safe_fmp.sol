//@ compile-flags: -Ogas --emit=bin

// Memory-safe assembly still observes allocation addresses. Its raw loads and the summaries of
// helpers that return them must retain free-pointer provenance across scratch reuse.
contract MemorySafeLoads {
    function unknownSlot(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        // Both candidate slots are valid to read, and one is the free-memory-pointer slot.
        assembly ("memory-safe") { pointer := mload(and(slot, 0x40)) }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function throughHelper(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer = forwardPointer(slot);
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function calleeScratch(uint256 slot) external pure returns (uint256 result) {
        uint256 pointer;
        assembly ("memory-safe") { pointer := mload(and(slot, 0x40)) }
        require(temporary() != bytes32(0));
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function readPointer(uint256 slot) internal pure returns (uint256 pointer) {
        /// @solidity memory-safe-assembly
        assembly { pointer := mload(and(slot, 0x40)) }
    }

    function forwardPointer(uint256 slot) internal pure returns (uint256) {
        return readPointer(slot);
    }

    function copiedParameter(uint256 base) external view returns (uint256 result) {
        uint256 pointer = copyWord(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(pointer, 32)) }
    }

    function copyWord(uint256 base) internal view returns (uint256 word) {
        assembly ("memory-safe") {
            pop(staticcall(gas(), 4, add(base, 32), 32, 0, 32))
            word := mload(0)
        }
    }

    function wrappedCopy(uint256 offset) external view returns (uint256 result) {
        bytes memory payload = abi.encode(uint256(0));
        uint256 pointer = copyOffset(payload, offset); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(pointer, 32)) }
    }

    function copyOffset(bytes memory payload, uint256 offset) internal view returns (uint256 word) {
        assembly ("memory-safe") {
            pop(staticcall(gas(), 4, add(payload, offset), 32, 0, 32))
            word := mload(0)
        }
    }

    /// @custom:solar-noinline
    function temporary() internal pure returns (bytes32 digest) {
        /// @custom:solar-scratch
        { digest = keccak256(abi.encode(uint256(101))); }
    }
}

// Reserving memory by increasing the free pointer does not end the lifetime of aliases of the
// installed value. Neither annotations nor helper calls may hide those aliases or partial writes.
contract MemorySafeInstalls {
    function installed(uint256 base) external pure returns (uint256 result) {
        assembly ("memory-safe") {
            if lt(base, mload(0x40)) { revert(0, 0) }
            mstore(0x40, base)
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function helperInstall(uint256 base) external pure returns (uint256 result) {
        forwardInstall(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function calleeInstalls(uint256 base) external pure returns (uint256 result) {
        require(installAndReuse(base) != bytes32(0)); //~ ERROR: this passes a free memory pointer to a function that reuses scratch memory
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function partialInstall(uint256 lowByte) external pure returns (uint256 result) {
        uint256 base = 65536 + (lowByte & 0xff);
        assembly ("memory-safe") {
            mstore(0x40, 65536)
            mstore8(95, lowByte) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) }
    }

    function copiedInstall(uint256 lowByte) external pure returns (uint256 result) {
        uint256 base = 65536 + (lowByte & 0xff);
        assembly ("memory-safe") {
            mstore(0, base)
            mcopy(0x40, 0, 32) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) }
    }

    function install(uint256 base) internal pure {
        assembly ("memory-safe") {
            if lt(base, mload(0x40)) { revert(0, 0) }
            mstore(0x40, base)
        }
    }

    function forwardInstall(uint256 base) internal pure {
        install(base);
    }

    /// @custom:solar-noinline
    function installAndReuse(uint256 base) internal pure returns (bytes32 digest) {
        assembly ("memory-safe") {
            if lt(base, mload(0x40)) { revert(0, 0) }
            mstore(0x40, base)
        }
        /// @custom:solar-scratch
        { digest = keccak256(abi.encode(uint256(101))); }
    }
}

// Retyping a function pointer touches no memory. The resulting dispatcher can nevertheless pass
// a plain word to a typed helper as a memory reference, so its callees remain conservative too.
contract MemorySafeRetyped {
    function beforeScratch() external pure returns (uint256 result) {
        function(uint256[] memory, uint256) internal pure typed = writeArray;
        function(uint256, uint256) internal pure retyped;
        assembly ("memory-safe") {
            retyped := typed
            mstore(32, 1)
        }
        // The forged array's first element is the free-memory-pointer word.
        retyped(32, 65536); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(65568) }
    }

    function scratchInTypedCallee() external pure returns (uint256 result) {
        function(uint256[] memory, uint256) internal pure returns (bytes32) typed = writeAndReuse;
        function(uint256, uint256) internal pure returns (bytes32) retyped;
        assembly ("memory-safe") {
            retyped := typed
            mstore(32, 1)
        }
        require(retyped(32, 65536) != bytes32(0)); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(65568) }
    }

    function writeArray(uint256[] memory array, uint256 base) internal pure {
        array[0] = base;
    }

    function writeAndReuse(uint256[] memory array, uint256 base)
        //~^ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        internal
        pure
        returns (bytes32 digest)
    {
        array[0] = base; //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { digest = keccak256(abi.encode(uint256(101))); }
    }
}
