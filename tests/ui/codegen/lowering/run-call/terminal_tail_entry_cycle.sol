//@ codegen-matrix: standard raw
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped,cfg-simplify
//@[raw] filecheck: --implicit-check-not=mload
//@ run-call-fail: AcyclicClosureBridge::enter 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: CyclicClosureBridge::enter 0 => 0x0000000000000000000000000000000000000000000000000000000000000000

// cfg-simplify removes the source loop's trivial Phi so the closure cycle guard is reached.
// CHECK-LABEL: @module AcyclicClosureBridge_runtime
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: dup 2
// CHECK-NEXT: eq
// CHECK-NEXT: iszero
// CHECK-NEXT: jumpi [[SIDE:bb[0-9]+]], [[DONE:bb[0-9]+]]
// CHECK: [[SIDE]] [cold]:
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[DONE]]
// CHECK: [[DONE]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
contract AcyclicClosureBridge {
    function enter(uint256 a) external pure { finish(a); revert(); }
    function finish(uint256 a) internal pure {
        assembly {
            if a { mstore(0, a) }
            mstore(0, a)
            revert(0, 32)
        }
    }
}
// CHECK-LABEL: @module CyclicClosureBridge_runtime
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 192
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[LOOP:bb[0-9]+]]
// CHECK: [[LOOP]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: dup 2
// CHECK-NEXT: eq
// CHECK-NEXT: iszero
// CHECK-NEXT: jumpi [[BODY:bb[0-9]+]], [[EXIT:bb[0-9]+]]
// CHECK: [[EXIT]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
// CHECK: [[BODY]]:
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[LOOP]]
contract CyclicClosureBridge {
    function enter(uint256 a) external pure { finish(a); revert(); }
    function finish(uint256 a) internal pure {
        assembly {
            for {} a {} { mstore(0, a) }
            mstore(0, a)
            revert(0, 32)
        }
    }
}
