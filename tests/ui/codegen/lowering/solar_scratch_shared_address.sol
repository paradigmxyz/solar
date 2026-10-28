//@ compile-flags: -Ogas --emit=bin

// A small arithmetic DAG must not expand into an exponential expression tree while checking
// scratch lifetimes. Both the load and the store use the same computed FMP address: the store
// is allowed, but the pointer must still be rejected when it outlives the scratch block.
contract Test {
    function sharedAddress() external pure returns (uint256 result) {
        uint256 pointer;
        assembly {
            let p := 0
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, p)
            p := add(p, 64)
            pointer := mload(p)
            mstore(p, pointer)
        }
        /// @custom:solar-scratch
        { bytes memory temporary = abi.encode(uint256(101)); }
        assembly { result := mload(pointer) } //~ ERROR: memory of a `@custom:solar-scratch` block is used after the block
    }
}
