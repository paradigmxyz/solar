//@ codegen-matrix: standard
//@[gas,size] compile-flags: -Zdump=evm-ir-runtime
//@[gas,size] filecheck:
//@ run-call: MultipleFmpFrontiers::s => 0
//@ run-call: MultipleFmpFrontiers::left => true
//@ run-call: MultipleFmpFrontiers::right => true

// Both external closures need FMP; retain exactly one global initializer.
// The only literal 160 is the initial heap floor; reject a second initializer.
// CHECK-LABEL: @module MultipleFmpFrontiers_runtime
// CHECK: push 160
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK-NOT: push 160
// CHECK: push 224
// CHECK-NEXT: shr
// CHECK-NOT: push 160
contract MultipleFmpFrontiers {
    uint256 public s;
    function left() external pure returns (bool result) {
        assembly { result := iszero(iszero(mload(64))) }
    }
    function right() external pure returns (bool result) {
        assembly { result := gt(mload(64), 63) }
    }
}
