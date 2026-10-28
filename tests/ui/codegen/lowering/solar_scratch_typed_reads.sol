//@ compile-flags: -Ogas --emit=bin

// Memory loads at unknown addresses only count as possible free memory pointer reads in a
// contract with inline assembly, the only way to point at the pointer's slot. Without it, a typed
// read through a pointer parameter, such as `xs.length`, leaves words free to leave the block.
contract TypedReads {
    function typed(uint256[] memory xs, uint256 w) external pure returns (uint256) {
        uint256 v = w + xs.length + xs[1];
        bytes32 digest;
        /// @custom:solar-scratch
        {
            digest = keccak256(abi.encode(v));
        }
        return v + 1 + uint256(digest);
    }
}

// Inline assembly anywhere in the contract may have forged a pointer to the slot, so the same code
// is rejected there.
contract WithAssembly {
    function typed(uint256[] memory xs, uint256 w) external pure returns (uint256) {
        uint256 v = w + xs.length;
        bytes32 digest;
        /// @custom:solar-scratch
        {
            digest = keccak256(abi.encode(v)); //~ ERROR: this stores a reference to memory of a `@custom:solar-scratch` block where code after the block can reach it
        }
        return v + 1 + uint256(digest); //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }

    function assembly_() external pure returns (uint256 x) {
        assembly { x := 1 }
    }
}
