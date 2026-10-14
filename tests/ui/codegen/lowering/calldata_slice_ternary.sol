//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CalldataSliceTernary {
    // A calldata-typed ternary merges lazily: each arm's pointer and length
    // round-trip through scratch and re-form a slice, with no calldata copy.
    // CHECK-LABEL: fn @pick{{[( ]}}
    // CHECK-NOT: copy calldata
    // CHECK: phi [
    // CHECK: slice_len
    // CHECK-NOT: copy calldata
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
    // CHECK: [[COPY:v[0-9]+]] = alloc {{v[0-9]+}}
    // CHECK-NEXT: store [[COPY]], {{v[0-9]+}}
    // CHECK-NEXT: [[DATA:v[0-9]+]] = ptradd [[COPY]], 32
    // CHECK-NEXT: [[SRC:v[0-9]+]] = slice_ptr arg1
    // CHECK-NEXT: [[LEN:v[0-9]+]] = slice_len arg1
    // CHECK-NEXT: copy calldata [[DATA]], [[SRC]], [[LEN]]
    // CHECK: phi [bb{{[0-9]+}}: [[COPY]]], [bb{{[0-9]+}}: {{v[0-9]+}}]
    function adopt(bool c, bytes calldata a) external pure returns (bytes memory) {
        bytes memory local = hex"aabb";
        return c ? a : local;
    }
}
