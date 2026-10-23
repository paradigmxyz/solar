//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
// `@custom:solar-optimize size` makes optimized builds compile a contract for size: its MIR module
// carries the objective, while an untagged contract keeps the build's.

// CHECK-LABEL: @module Small
// CHECK-NEXT: @phase lowered
// CHECK-NEXT: @optimize size
/// @custom:solar-optimize size
contract Small {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}

// CHECK-LABEL: @module Plain
// CHECK-NOT: @optimize
// CHECK: fn
contract Plain {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
