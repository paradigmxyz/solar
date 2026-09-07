//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped

//@ run-call-fail: fast 7 => 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: blocked 7 => 0x0000000000000000000000000000000000000000000000000000000000000008

// CHECK-LABEL: @module TerminalTailEntryMixed_runtime
// CHECK: gas
// CHECK-NEXT: push 128
// CHECK-NEXT: mstore
// CHECK: push 480
// CHECK-NEXT: mstore
// CHECK: push 128
// CHECK-NEXT: mload
// CHECK: push 576
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[BODY:bb[0-9]+]]
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[BODY]]
// CHECK: [[BODY]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert

contract TerminalTailEntryMixed {
    function fast(uint256 a) external pure { fail(a); }
    function blocked(uint256 a) external view {
        uint256 result;
        assembly {
            let v0 := gas()
            let v1 := gas()
            let v2 := gas()
            let v3 := gas()
            let v4 := gas()
            let v5 := gas()
            let v6 := gas()
            let v7 := gas()
            let v8 := gas()
            let v9 := gas()
            let v10 := gas()
            let v11 := gas()
            let v12 := gas()
            let v13 := gas()
            let v14 := gas()
            let v15 := gas()
            let v16 := gas()
            let v17 := gas()
            let v18 := gas()
            let v19 := gas()
            result := add(a, gt(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(v0, v1), v2), v3), v4), v5), v6), v7), v8), v9), v10), v11), v12), v13), v14), v15), v16), v17), v18), v19), 0))
        }
        fail(result);
    }
    function fail(uint256 a) internal pure {
        assembly { mstore(0, a) revert(0, 32) }
    }
}
