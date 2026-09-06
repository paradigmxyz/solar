//@ codegen-matrix: standard
//@[gas,size] compile-flags: -Zdump=evm-ir-runtime
//@[gas,size] filecheck:
//@ run-call: UniqueFmpFrontier::s => 0
//@ run-call: UniqueFmpFrontier::initialized => true

// Runtime IR must have exactly one existing FMP initializer at initialized's entry,
// before entry argument/spill materialization; the getter dispatch path must bypass it.
// The only literal 160 is the initial heap floor; reject a second initializer.
// CHECK-LABEL: @module UniqueFmpFrontier_runtime
// CHECK-NOT: push 64
// CHECK: push 224
// CHECK-NEXT: shr
// CHECK: push 160
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK-NOT: push 160
contract UniqueFmpFrontier {
    uint256 public s;
    function initialized() external pure returns (bool result) {
        assembly { result := iszero(iszero(mload(64))) }
    }
}
