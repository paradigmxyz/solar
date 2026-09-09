//@ codegen-matrix: standard ir raw
//@[ir] filecheck: --implicit-check-not=mload
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck: --implicit-check-not=mload
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: gasValue 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: memorySize 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001

// CHECK-LABEL: @module TerminalTailEntryPrefix_runtime
// CHECK: push 256
// CHECK-NEXT: push 64
// CHECK-NEXT: mstore
// CHECK: msize
// CHECK: jump [[BODY:bb[0-9]+]]
// CHECK: gas
// CHECK: jump [[BODY]]
// CHECK: [[BODY]] [cold]:
// CHECK: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 64
// CHECK-NEXT: push 0
// CHECK-NEXT: revert

contract TerminalTailEntryPrefix {
    function gasValue(uint256 a) external view {
        uint256 observed;
        assembly { observed := gt(gas(), 0) }
        fail(a, observed);
    }
    function memorySize(uint256 a) external pure {
        uint256 observed;
        assembly { mstore(0, 5) observed := gt(msize(), 0) }
        fail(a, observed);
    }
    function fail(uint256 a, uint256 observed) internal pure {
        assembly { mstore(0, a) mstore(32, observed) revert(0, 64) }
    }
}
