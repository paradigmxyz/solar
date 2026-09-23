//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: masked 9, 4 => 13, 13, 0
//@ run-call: masked 7, 7 => 7, 0, 7
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 1

contract ResidentWords {
    // Each argument is read from calldata once and stays resident for its next uses, so
    // `x | y` reads the copies directly. This is as cheap as rebuilding it from the resident
    // results, x | y => (x & y) + (x ^ y), which only pays when the arguments must be reloaded.
    // CHECK-LABEL: @module ResidentWords_runtime
    // CHECK: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NOT: calldataload
    // CHECK: and
    // CHECK-NOT: calldataload
    // CHECK: or
    function masked(uint256 x, uint256 y)
        external pure returns (uint256 either, uint256 different, uint256 common)
    {
        either = x | y;
        different = x ^ y;
        common = x & y;
    }
}
