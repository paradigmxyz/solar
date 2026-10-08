//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: masked 9, 4 => 46
//@ run-call: masked 7, 7 => 27
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 1

contract ResidentWords {
    // Reuse both earlier results while they stay resident: x | y => (x & y) + (x ^ y).
    // CHECK-LABEL: @module ResidentWords_runtime
    // CHECK: and
    // CHECK-NOT: {{^ +or$}}
    // CHECK-DAG: xor
    // CHECK-DAG: add
    // CHECK-NOT: {{^ +or$}}
    // CHECK: return
    function masked(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 different = x ^ y;
        uint256 common = x & y;
        uint256 either = x | y;
        return (either << 2) ^ (different << 1) ^ common;
    }
}
