//@ codegen-matrix: standard ir raw
//@[ir] filecheck: --implicit-check-not=mload
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck: --implicit-check-not=mload
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: forward 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000010
//@ run-call-fail: backward 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000010
//@ run-call-fail: repeated 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000012

// CHECK-LABEL: @module TerminalTailEntryUnused_runtime
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK: push 36
// CHECK-NEXT: calldataload
// CHECK: jump [[BODY:bb[0-9]+]]
// CHECK: push 36
// CHECK-NEXT: calldataload
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK: jump [[BODY]]
// CHECK: [[BODY]] [cold]:
// CHECK: dup 2
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: add
// CHECK-NEXT: push 64
// CHECK-NEXT: mstore
// CHECK-NEXT: push 96
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
// CHECK: dup 1
// CHECK-NEXT: jump [[BODY]]

contract TerminalTailEntryUnused {
    function forward(uint256 a, uint256 b) external pure { fail(a, 777, b); }
    function backward(uint256 a, uint256 b) external pure { fail(b, 777, a); }
    function repeated(uint256 a) external pure { fail(a, 777, a); }
    function fail(uint256 a, uint256 unused, uint256 b) internal pure {
        assembly {
            mstore(0, a)
            mstore(32, b)
            mstore(64, add(a, b))
            revert(0, 96)
        }
    }
}
