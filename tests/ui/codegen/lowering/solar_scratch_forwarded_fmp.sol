//@ codegen-matrix: standard

// Storing a scalar into the FMP word makes the reloaded scalar a free pointer. Sharing the
// slot expression ensures store forwarding sees the same address value for both accesses.
contract Test {
    function computedSlot() external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            let slot := add(32, 32)
            mstore(slot, 0x400)
            pointer := mload(slot)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function runtimeSlot(uint256 slot, uint256 base) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(slot, base)
            pointer := mload(slot)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function overlappingSlot() external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(64, 0)
            let slot := add(64, 1)
            mstore(slot, shl(8, 0x400))
            pointer := shr(8, mload(slot))
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function resetAndRead(uint256 base) internal pure returns (uint256 pointer) {
        assembly {
            let slot := add(32, 32)
            mstore(slot, base)
            pointer := mload(slot)
        }
    }

    function forwardReset(uint256 base) internal pure returns (uint256) {
        return resetAndRead(base);
    }

    function returnedThroughWrapper(uint256 base) external pure returns (uint256 result) {
        uint256 pointer = forwardReset(base);
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function stashReset(uint256 base) internal pure {
        assembly {
            let slot := add(32, 32)
            mstore(slot, base)
            mstore(0, mload(slot))
        }
    }

    function capturedByHelper(uint256 base) external pure returns (uint256 result) {
        stashReset(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mload(0), 32)) }
    }

    /// @custom:solar-noinline
    function temporary() internal pure returns (bytes32 digest) {
        /// @custom:solar-scratch
        { digest = keccak256(abi.encode(uint256(101))); }
    }

    function callerKeeps(uint256 slot, uint256 base) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(slot, base)
            pointer := mload(slot)
        }
        require(temporary() != bytes32(0));
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(pointer, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }
}
