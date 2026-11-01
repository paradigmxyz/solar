//@ compile-flags: -O none --optimize-runs 10000 -Zdump=mir
//@ filecheck:

// `@custom:solar-optimize` optimizes the code it documents even in a `-O none` build. A contract
// without the tag stays unoptimized, and so do the functions of a contract beside a tagged one.
// CHECK-LABEL: :Plain ===
// CHECK: phi
contract Plain {
    function sum(bytes calldata data) external pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 8) | uint8(data[i]);
        }
    }
}

// CHECK-LABEL: :Tagged ===
// CHECK-NOT: phi
// CHECK-LABEL: :Mixed ===
/// @custom:solar-optimize gas
contract Tagged {
    function sum(bytes calldata data) external pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 8) | uint8(data[i]);
        }
    }
}

contract Mixed {
    // CHECK-LABEL: fn @slow(
    // CHECK: phi
    function slow(bytes calldata data) external pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 8) | uint8(data[i]);
        }
    }

    // CHECK-LABEL: fn @fast(
    // CHECK-NOT: phi
    // CHECK: ret
    /// @custom:solar-optimize gas
    function fast(bytes calldata data) external pure returns (uint32 value) {
        for (uint256 i = 0; i < 4; i++) {
            value = (value << 7) | uint8(data[i]);
        }
    }
}
