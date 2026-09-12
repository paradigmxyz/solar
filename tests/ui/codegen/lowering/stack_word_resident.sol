//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: masked 9, 4 => 13, 13, 0
//@ run-call: masked 7, 7 => 7, 0, 7
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 1

contract ResidentWords {
    // Keep both earlier results resident: x | y => (x & y) + (x ^ y).
    // CHECK-LABEL: @module ResidentWords_runtime
    // CHECK: and
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    function masked(uint256 x, uint256 y)
        external pure returns (uint256 either, uint256 different, uint256 common)
    {
        either = x | y;
        different = x ^ y;
        common = x & y;
    }
}
