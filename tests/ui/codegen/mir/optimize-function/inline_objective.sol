//@ compile-flags: -Ogas --optimize-runs 10000 -Zdump=mir
//@ filecheck:

// A function inlined into a caller joins the caller's code, so the caller's objective optimizes
// it: loop unrolling, a gas transformation, copies the loop of a `size` helper cloned into a gas
// caller, and leaves the loop of a gas helper cloned into a `size` caller alone.
contract Inlined {
    /// @custom:solar-inline always
    /// @custom:solar-optimize size
    function small(bytes calldata data) internal pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 8) | uint8(data[i]);
        }
    }

    /// @custom:solar-inline always
    function fast(bytes calldata data) internal pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 7) | uint8(data[i]);
        }
    }

    // CHECK-LABEL: fn @gasCaller(
    // CHECK-NOT: icall @small
    // CHECK-NOT: phi
    // CHECK: ret
    function gasCaller(bytes calldata data) external pure returns (uint32) {
        return small(data);
    }

    // CHECK-LABEL: fn @sizeCaller(
    // CHECK-NOT: icall @fast
    // CHECK: phi
    // CHECK: ret
    /// @custom:solar-optimize size
    function sizeCaller(bytes calldata data) external pure returns (uint32) {
        return fast(data);
    }
}
