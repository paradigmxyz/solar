//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: find [], 7 => 0
//@ run-call: find [7], 7 => 0
//@ run-call: find [1], 7 => 1
//@ run-call: find [1, 7], 7 => 1
//@ run-call: find [1, 2], 7 => 2
//@ run-call: find [1, 2, 7], 7 => 2
//@ run-call: find [1, 2, 3, 4, 7], 7 => 4

contract JoinEdgeReload {
    // The loop never reads the length that the search returns when nothing
    // matches, so the header branches on its pointers alone. The length stays
    // on the stack in this loop; a loop that spills it, as an unrolled search
    // does, reloads it on the exit edge after the branch, only when it ends.
    // CHECK-LABEL: @module JoinEdgeReload_runtime
    // CHECK: [loop]:
    // CHECK: [loop]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: push bb{{[0-9]+}}
    // CHECK-NEXT: jumpi
    function find(uint256[] calldata values, uint256 x) external pure returns (uint256 found) {
        found = values.length;
        for (uint256 i = 0; i < values.length; ++i) {
            if (values[i] == x) {
                found = i;
                break;
            }
        }
    }
}
