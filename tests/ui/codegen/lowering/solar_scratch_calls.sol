//@ codegen-matrix: standard

// Scratch checks run before inlining. References held by callers and helpers must be checked
// even when the function containing the block never reads or returns those references.
contract Test {
    uint256 private savedPointer;

    /// @custom:solar-noinline
    function temporary(uint256 x) internal pure returns (bytes32 digest) {
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            digest = keccak256(encoded);
        }
    }

    function callerKeeps(uint256 x) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        require(temporary(x) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    /// @custom:solar-noinline
    function usesArgument(uint256 pointer, uint256 x) internal pure returns (uint256 result) {
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) }
    }

    function callerPasses(uint256 x) external pure returns (uint256) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        return usesArgument(pointer, x); //~ ERROR: this passes a free memory pointer to a function that reuses scratch memory
    }

    /// @custom:solar-noinline
    function stashMemory() internal pure {
        assembly { mstore(0, mload(0x40)) }
    }

    function helperStashesMemory(uint256 x) external pure returns (uint256 result) {
        stashMemory(); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(mload(0), 0x20)) }
    }

    /// @custom:solar-noinline
    function stashStorage() internal {
        assembly { sstore(savedPointer.slot, mload(0x40)) }
    }

    function helperStashesStorage(uint256 x) external returns (uint256 result) {
        stashStorage(); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(sload(savedPointer.slot), 0x20)) }
    }

    function wrapper(uint256 x) internal pure returns (bytes32) {
        return temporary(x);
    }

    function throughWrapper(uint256 x) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        require(wrapper(x) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function recursiveTemporary(uint256 x, uint256 depth) internal pure returns (bytes32) {
        if (depth == 0) return temporary(x);
        return recursiveTemporary(x, depth - 1);
    }

    function throughRecursion(uint256 x) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        require(recursiveTemporary(x, 2) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function throughPointer(uint256 x) external pure returns (uint256 result) {
        function(uint256) internal pure returns (bytes32) callback = temporary;
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        require(callback(x) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function recursiveStash(uint256 depth) internal pure {
        if (depth == 0) stashMemory();
        else recursiveStash(depth - 1);
    }

    function transitiveStash(uint256 x, bool store) external pure returns (uint256 result) {
        if (store) recursiveStash(2); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(mload(0), 0x20)) }
    }

    function acrossLoop(uint256 x, uint256 n) external pure returns (uint256 result) {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        for (uint256 i; i < n; ++i) require(temporary(x + i) != bytes32(0));
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        assembly { result := mload(add(pointer, 0x20)) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    /// @custom:solar-noinline
    function stashInArray(uint256[] memory saved) internal pure {
        uint256 pointer;
        assembly { pointer := mload(0x40) }
        saved[0] = pointer;
    }

    function helperStashesArray(uint256 x) external pure returns (uint256 result) {
        uint256[] memory saved = new uint256[](1);
        stashInArray(saved); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        /// @custom:solar-scratch
        {
            bytes memory encoded = abi.encode(x);
            require(keccak256(encoded) != bytes32(0));
        }
        bytes memory later = abi.encode(x + 2);
        require(keccak256(later) != bytes32(0));
        uint256 pointer = saved[0];
        assembly { result := mload(add(pointer, 0x20)) }
    }
}
