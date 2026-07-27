//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract CalldataSliceTernary {
    // A calldata-typed ternary merges lazily: each arm's pointer and length
    // round-trip through scratch and re-form a slice, with no calldata copy.
    // CHECK-LABEL: fn @pick{{[( ]}}
    // CHECK-NOT: calldatacopy
    // CHECK: slice_ptr
    // CHECK-NOT: calldatacopy
    // CHECK: slice_len
    // CHECK-NOT: calldatacopy
    // CHECK: make_calldata_slice
    // CHECK-NOT: calldatacopy
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
    // CHECK-LABEL: fn @adopt{{[( ]}}
    // CHECK: calldatacopy
    function adopt(bool c, bytes calldata a) external pure returns (bytes memory) {
        bytes memory local = hex"aabb";
        return c ? a : local;
    }
}
