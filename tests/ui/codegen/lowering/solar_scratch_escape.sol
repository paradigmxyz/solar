//@ compile-flags: -Ogas --emit=bin

// Memory a `@custom:solar-scratch` block allocates is reused after the block,
// so no reference to it may reach code after the block: not through a
// variable, a return, a store into older memory, or a callee. Values such as
// hashes and lengths leave the block freely.
contract Test {
    bytes stored;

    function usedAfter(uint256 x) external pure returns (uint256) {
        bytes memory kept;
        /// @custom:solar-scratch
        {
            kept = abi.encode(x);
        }
        return kept.length; //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function throughReturnVariable(uint256 x) external pure returns (bytes memory b) { //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
        /// @custom:solar-scratch
        {
            bytes memory t = abi.encode(x);
            if (t.length == 32) b = t;
        }
    }

    function returned(uint256 x) external pure returns (bytes memory) {
        /// @custom:solar-scratch
        {
            bytes memory t = abi.encode(x);
            return t; //~ ERROR: this returns memory of a `@custom:solar-scratch` block
        }
    }

    function storedOutside(uint256 x, bytes[] memory outer) external pure returns (uint256) {
        /// @custom:solar-scratch
        {
            bytes memory t = abi.encode(x);
            outer[0] = t; //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        return outer.length;
    }

    function link(bytes[] memory into, uint256 x) internal pure {
        into[0] = abi.encode(x);
    }

    function viaCall(uint256 x, bytes[] memory outer) external pure returns (uint256) {
        /// @custom:solar-scratch
        {
            link(outer, x); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        return outer.length;
    }

    function stash(uint256 x) internal pure returns (uint256 word) {
        assembly {
            word := add(mload(0x40), x)
        }
    }

    function viaAssembly(uint256 x) external pure returns (uint256 y) {
        /// @custom:solar-scratch
        {
            y = stash(x); //~ ERROR: this call may store a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
    }

    // The next iteration reads memory this one's block already reused.
    function loopCarried(uint256 n) external pure returns (uint256 total) {
        bytes memory previous;
        for (uint256 i; i < n; ++i) {
            /// @custom:solar-scratch
            {
                bytes memory t = abi.encode(i);
                total += previous.length; //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
                previous = t;
            }
        }
    }

    function valuesLeave(uint256 x, bytes[] memory outer) external returns (bytes32 h, uint256 n) {
        /// @custom:solar-scratch
        {
            bytes memory t = abi.encode(x, x);
            bytes[] memory local = new bytes[](1);
            local[0] = t;
            link(local, x);
            h = keccak256(local[0]);
            n = t.length + outer.length;
            stored = t;
        }
    }
}
