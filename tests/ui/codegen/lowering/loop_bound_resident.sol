//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: accumulate 7, 11, 0 => 7
//@ run-call: accumulate 7, 11, 1 => 18
//@ run-call: accumulate 7, 11, 4 => 55
//@ run-call: accumulate 7, 11, 64 => 42375
//@ run-call: accumulate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 1 => 0

contract LoopBoundResident {
    // CHECK-LABEL: @module LoopBoundResident_runtime
    // The header duplicates both the resident bound and the current counter.
    // CHECK: dup 4
    // CHECK-NEXT: dup 4
    // CHECK-NEXT: lt
    function accumulate(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := add(x, y)
                y := add(y, i)
            }
        }
        return x;
    }
}
