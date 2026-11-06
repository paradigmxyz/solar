//@ compile-flags: -Zdump=evm-ir-runtime-scheduled,evm-ir-runtime
//@ filecheck:

// `-Zdump=evm-ir-runtime-scheduled` prints the runtime EVM IR as stack scheduling emits it,
// before the EVM IR pipeline, which still jumps between the blocks that the pipeline merges.

// CHECK-LABEL: (runtime) ===
// CHECK: @module C_runtime
// CHECK-LABEL: (scheduled runtime) ===
// CHECK: @module C_runtime
// CHECK: bb0:
// CHECK: jump bb
contract C {
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
