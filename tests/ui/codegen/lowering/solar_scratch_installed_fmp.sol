//@ compile-flags: -O none --emit=bin

contract Test {
    function retained(uint256 base) external pure returns (bytes32 hash, uint256 result) {
        assembly { mstore(0x40, base) }
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(uint256(101));
            hash = keccak256(encoded);
        }
        bytes memory next = abi.encode(uint256(103));
        hash ^= keccak256(next);
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function siblingAlias(uint256 base) external pure returns (uint256 result) {
        uint256 alias_;
        assembly {
            alias_ := add(base, 64)
            mstore(0x40, add(base, 32))
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(alias_) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function constantAlias() external pure returns (uint256 result) {
        uint256 base = 65536;
        assembly { mstore(0x40, add(base, 32)) }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 64)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function scaledConstantAlias() external pure returns (uint256 result) {
        uint256 base = 7;
        assembly { mstore(0x40, mul(base, 65536)) }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mul(base, 65536), 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function resetThroughLoad(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(64, base)
            mstore(64, mload(64))
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function computedPointerSlot(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(64, 64)
            let pointer := mload(add(32, 32))
            mstore(pointer, base) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function capturedBeforeInstall(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(0, base) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
            mstore(0x40, base)
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mload(0), 32)) }
    }

    function dynamicSlot(uint256 slot, uint256 base) external pure returns (uint256 result) {
        assembly { mstore(slot, base) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function dynamicResetThroughLoad(uint256 slot, uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(slot, base) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
            mstore(64, mload(64))
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function partialInstall(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(64, 0)
            mstore8(94, base) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(shl(8, base), 32)) }
    }

    function copiedInstall(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(0, base)
            mcopy(0x40, 0, 32) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function install(uint256 base) internal pure {
        assembly { mstore(0x40, base) }
    }

    function installThroughWrapper(uint256 base) internal pure {
        install(base);
    }

    function identity(uint256 base) internal pure returns (uint256) {
        return base;
    }

    function forwardedInstall(uint256 base) external pure returns (uint256 result) {
        uint256 pointer = identity(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        assembly { mstore(0x40, pointer) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function helperInstall(uint256 base) external pure returns (uint256 result) {
        installThroughWrapper(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function installAndReuse(uint256 base) internal pure returns (bytes32 hash) {
        assembly { mstore(0x40, base) }
        /// @custom:solar-scratch
        { hash = keccak256(abi.encode(uint256(101))); }
    }

    function callerKeeps(uint256 base) external pure returns (uint256 result) {
        require(installAndReuse(base) != bytes32(0)); //~ ERROR: this passes a free memory pointer to a function that reuses scratch memory
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function installReuseAndReset(uint256 base) internal pure returns (bytes32 hash) {
        assembly { mstore(0x40, base) }
        /// @custom:solar-scratch
        { hash = keccak256(abi.encode(uint256(101))); }
        hash ^= keccak256(abi.encode(uint256(103)));
        assembly { mstore(0x40, 0x800) }
    }

    function callerAfterReset(uint256 base) external pure returns (uint256 result) {
        require(installReuseAndReset(base) != bytes32(0)); //~ ERROR: this passes a free memory pointer to a function that reuses scratch memory
        assembly { result := mload(add(base, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function writeArray(uint256[] memory array, uint256 base) internal pure {
        array[0] = base;
    }

    function forgedDestination(uint256 base) external pure returns (uint256 result) {
        uint256[] memory array;
        assembly {
            array := 32
            mstore(32, 1)
        }
        writeArray(array, base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function stashAndInstall(uint256 base) internal pure {
        assembly {
            mstore(0, base)
            mstore(0x40, base)
        }
    }

    function helperCaptures(uint256 base) external pure returns (uint256 result) {
        stashAndInstall(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(mload(0), 32)) }
    }

    function loopAlias(uint256 unused) external pure returns (uint256 result) {
        uint256 retained;
        for (uint256 i; i < 2; ++i) {
            uint256 base;
            assembly { base := calldataload(4) }
            if (i != 0) {
                assembly { result := mload(add(retained, 32)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
                return result;
            }
            retained = base;
            assembly { mstore(0x40, base) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
            /// @custom:solar-scratch
            { bytes memory encoded = abi.encode(uint256(101)); }
            bytes memory next = abi.encode(uint256(103));
        }
    }
}
