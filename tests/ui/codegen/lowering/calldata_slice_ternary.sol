//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=SLICE

contract CalldataSliceTernary {
    // A calldata-typed ternary merges lazily: each arm's pointer and length
    // round-trip through scratch and re-form a slice, with no calldata copy.
    function pick(bool c, bytes calldata a, bytes calldata b)
        external
        pure
        returns (uint256)
    {
        bytes calldata chosen = c ? a : b;
        return chosen.length;
    }

    // A memory-typed ternary adopts a calldata arm by materializing it, so
    // the merge stays a single memory pointer.
    function adopt(bool c, bytes calldata a) external pure returns (bytes memory) {
        bytes memory local = hex"aabb";
        return c ? a : local;
    }
}

// SLICE-LABEL: fn @pick
// SLICE: slice_ptr
// SLICE: slice_len
// SLICE: make_calldata_slice
// SLICE-NOT: calldatacopy
// SLICE-LABEL: fn @adopt
// SLICE: calldatacopy
