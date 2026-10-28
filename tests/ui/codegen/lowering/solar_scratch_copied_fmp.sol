//@ codegen-matrix: standard

// Copying the free-pointer bytes captures them even without an SSA load from slot 0x40.
contract Test {
    function copied(uint256 a, uint256 b) external pure returns (uint256 result, bool same) {
        uint256 pointer;
        assembly {
            mcopy(0, 0x40, 32) //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
            pointer := mload(0)
        }
        bytes32 first;
        /// @custom:solar-scratch
        { first = keccak256(abi.encode(a)); }
        bytes memory next = abi.encode(b);
        same = first == keccak256(next);
        assembly { result := mload(add(pointer, 32)) }
    }

    function identity(uint256 a, uint256 b) external view returns (uint256 result, bool same) {
        uint256 pointer;
        assembly {
            if iszero(staticcall(gas(), 4, 0x40, 32, 0, 32)) { revert(0, 0) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
            pointer := mload(0)
        }
        bytes32 first;
        /// @custom:solar-scratch
        { first = keccak256(abi.encode(a)); }
        bytes memory next = abi.encode(b);
        same = first == keccak256(next);
        assembly { result := mload(add(pointer, 32)) }
    }

    function returnedData(uint256 a) external view returns (uint256 result) {
        assembly {
            if iszero(staticcall(gas(), 4, 0x40, 32, 0, 0)) { revert(0, 0) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        }
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
        assembly {
            returndatacopy(0, 0, 32)
            result := mload(add(mload(0), 32))
        }
    }

    function copyPointer() internal pure {
        assembly { mcopy(0, 0x40, 32) }
    }

    function forwardCopy() internal pure { copyPointer(); }

    function capturedThroughWrapper(uint256 a) external pure returns (uint256 result) {
        forwardCopy(); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
        assembly { result := mload(add(mload(0), 32)) }
    }

    function identityPointer() internal view returns (uint256 result) {
        assembly {
            if iszero(staticcall(gas(), 4, 0x40, 32, 0, 32)) { revert(0, 0) }
            result := mload(0)
        }
    }

    function returnedCopy(uint256 a) external view returns (uint256 result) {
        uint256 pointer = identityPointer(); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
        assembly { result := mload(add(pointer, 32)) }
    }

    /// @custom:solar-noinline
    function temporary(uint256 a) internal pure returns (bytes32 result) {
        /// @custom:solar-scratch
        { result = keccak256(abi.encode(a)); }
    }

    function callerCopies(uint256 a) external pure returns (uint256 result) {
        assembly { mcopy(0, 0x40, 32) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        require(temporary(a) != bytes32(0));
        assembly { result := mload(add(mload(0), 32)) }
    }

    function overlappingBytes(uint256 a) external pure returns (uint256 result) {
        // The range starts before the slot and includes the entire FMP word.
        assembly { mcopy(0, 63, 33) } //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
        assembly { result := mload(add(mload(1), 32)) }
    }
}
