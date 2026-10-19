//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:

// CHECK-LABEL: @module WithFallback
// CHECK: tail_call @a
contract WithFallback {
    function a(uint256 x) external pure returns (uint256) { return x + 1; }
    function b(uint256 x) external pure returns (uint256) { return x * 2; }
    fallback() external { }
}

// CHECK-LABEL: @module WithGas
// CHECK: tail_call @a
contract WithGas {
    function a(uint256 x) external view returns (uint256) { return x + gasleft(); }
    function b(uint256 x) external pure returns (uint256) { return x * 2; }
}

// CHECK-LABEL: @module WithDynamicInputs
// CHECK: tail_call @a
contract WithDynamicInputs {
    function a(uint256[] calldata x) external pure returns (uint256[] memory) { return x; }
    function b(uint256 x) external pure returns (uint256) { return x * 2; }
}

// CHECK-LABEL: @module NarrowInputs
// CHECK: tail_call @a
contract NarrowInputs {
    function a(uint8 x) external pure returns (uint256) { return x; }
    function b(uint256 x) external pure returns (uint256) { return x * 2; }
}
