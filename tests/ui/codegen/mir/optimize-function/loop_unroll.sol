//@ revisions: gas size
//@ compile-flags: --optimize-runs 10000 -Zdump=mir
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] filecheck: --check-prefixes=CHECK,SIZE

// `@custom:solar-optimize` on a function overrides the build's objective for that function's code:
// loop unrolling is a gas transformation, so it copies the body of a loop in a `gas` function even
// in a size build, and leaves the loop of a `size` function alone in a gas build.
contract Mixed {
    // CHECK-LABEL: fn @plain(
    // GAS-NOT: phi
    // SIZE: phi
    // CHECK: ret
    /// @custom:solar-inline never
    function plain(bytes calldata data) internal pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 8) | uint8(data[i]);
        }
    }

    // CHECK-LABEL: fn @fast(
    // CHECK-NOT: phi
    // CHECK: ret
    /// @custom:solar-inline never
    /// @custom:solar-optimize gas
    function fast(bytes calldata data) internal pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 7) | uint8(data[i]);
        }
    }

    // CHECK-LABEL: fn @small(
    // CHECK: phi
    // CHECK: ret
    /// @custom:solar-inline never
    /// @custom:solar-optimize size
    function small(bytes calldata data) internal pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 6) | uint8(data[i]);
        }
    }

    function run(bytes calldata data) external pure returns (uint32, uint32, uint32) {
        return (plain(data), fast(data), small(data));
    }
}
