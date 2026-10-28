//@ compile-flags: -O none --emit=bin

// Reloading an installed address must not hide aliases of the word originally stored. Literal
// addresses have distinct value identities, and writes and block boundaries cancel forwarding.
contract Test {
    function distinctLiterals(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(0, base)
            mstore(0x40, mload(0)) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function unrelatedWrite(uint256 base) external pure returns (uint256 result) {
        assembly {
            let slot := 0
            mstore(slot, base)
            mstore(0x20, 0)
            mstore(0x40, mload(slot)) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function crossBlock(uint256 base, bool write) external pure returns (uint256 result) {
        assembly { mstore(0, base) }
        if (write) {
            assembly { mstore(0x20, 0) }
        }
        assembly {
            mstore(0x40, mload(0)) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function computedReload(uint256 base) external pure returns (uint256 result) {
        assembly {
            mstore(0, base)
            mstore(0x20, 0)
            mstore(0x40, add(mload(0), 32)) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 64)) }
    }

    function helperInstalls(uint256 base) external pure returns (uint256 result) {
        installReloaded(base); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function forwardedInstaller(uint256 base) external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            mstore(0, base)
            mstore(0x20, 0)
            pointer := mload(0)
        }
        installThroughWrapper(pointer); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function returnedReload(uint256 base) external pure returns (uint256 result) {
        assembly { mstore(0, base) }
        uint256 pointer = readZero();
        assembly {
            mstore(0x40, pointer) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function forwardedReturn(uint256 base) external pure returns (uint256 result) {
        assembly { mstore(0, base) }
        uint256 pointer = readThroughWrapper();
        assembly {
            mstore(0x40, pointer) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly { result := mload(add(base, 32)) }
    }

    function installReloaded(uint256 base) internal pure {
        assembly {
            let slot := 0
            mstore(slot, base)
            mstore(0x20, 0)
            mstore(0x40, mload(slot))
        }
    }

    function install(uint256 pointer) internal pure {
        assembly { mstore(0x40, pointer) }
    }

    function installThroughWrapper(uint256 pointer) internal pure {
        install(pointer);
    }

    function readZero() internal pure returns (uint256 pointer) {
        assembly { pointer := mload(0) }
    }

    function readThroughWrapper() internal pure returns (uint256) {
        return readZero();
    }
}

// The same lifetime rule applies when every assembly block promises memory safety. The guard
// keeps the installed address at or above the current free memory pointer.
contract MemorySafe {
    function reloaded(uint256 base) external pure returns (uint256 result) {
        assembly ("memory-safe") {
            if lt(base, mload(0x40)) { revert(0, 0) }
            let slot := 0
            mstore(slot, base)
            mstore(0x20, 0)
            mstore(0x40, mload(slot)) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) }
    }

    function forwardedReturn(uint256 base) external pure returns (uint256 result) {
        assembly ("memory-safe") {
            if lt(base, mload(0x40)) { revert(0, 0) }
            mstore(0, base)
        }
        uint256 pointer = readThroughWrapper();
        assembly ("memory-safe") {
            mstore(0x40, pointer) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { bytes memory encoded = abi.encode(uint256(101)); }
        bytes memory next = abi.encode(uint256(103));
        assembly ("memory-safe") { result := mload(add(base, 32)) }
    }

    function readZero() internal pure returns (uint256 pointer) {
        assembly ("memory-safe") { pointer := mload(0) }
    }

    function readThroughWrapper() internal pure returns (uint256) {
        return readZero();
    }
}
