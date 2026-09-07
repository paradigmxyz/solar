//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: run 0 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: run 3 => 0x00000000000000000000000000000000000000000000000000000000000000f1

// CHECK-LABEL: @module TerminalTailEntryDynamic_runtime
// CHECK: push 0
// CHECK-NEXT: push 160
// CHECK-NEXT: mstore
// CHECK: {{push 7[[:space:]]+push 256[[:space:]]+mstore}}
// CHECK-NEXT: jump [[ENTRY:bb[0-9]+]]
// CHECK: [[ENTRY]] [cold]:
// CHECK-NEXT: push 256
// CHECK-NEXT: mload
// CHECK-NEXT: jump [[BODY:bb[0-9]+]]
// CHECK: [[BODY]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert

contract TerminalTailEntryDynamic {
    function run(uint256 depth) external pure {
        if (depth == 0) fail(7);
        fail(7 + recurse(depth, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12));
    }
    function recurse(uint256 depth, uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12) internal pure returns (uint256) {
        if (depth == 0) return 0;
        return (a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12) + recurse(depth - 1, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12);
    }
    function fail(uint256 a) internal pure {
        assembly { mstore(0, a) revert(0, 32) }
    }
}
