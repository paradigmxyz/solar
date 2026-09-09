//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,frame-slot-promotion,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call: missing 123 => 17
//@ run-call: missing 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 17
//@ run-call: firstOpcode false => 17
//@ run-call: firstOpcode true => 17
//@ run-call: successor false => 17, 34, 8721
//@ run-call: successor true => 34, 17, 4386

// Missing arguments and the first-opcode prefix stay canonical.
// The returning LOG body keeps both Phi values and the continuation below its operands.
// CHECK-LABEL: @module ResidentBoundaryControls_runtime
// CHECK: push 0xb114daa4
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[MISSING_ENTRY:bb[0-9]+]]
// CHECK-NEXT: [[MISSING_ENTRY]]:
// CHECK-NEXT: calldatasize
// CHECK-NEXT: push 36
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[MISSING:bb[0-9]+]]
// CHECK-NEXT: [[MISSING]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 170
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: dup 5
// CHECK-NEXT: swap 4
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 1
// CHECK-NEXT: log3
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: return
// CHECK-NEXT: [[CONT:bb[0-9]+]]:
// CHECK-NEXT: push 32
// CHECK-NEXT: mload
// CHECK: push 170
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: dup 4
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 1
// CHECK-NEXT: log2
// CHECK-NEXT: jump [[FIRST_JOIN:bb[0-9]+]]
// CHECK-NEXT: [[FIRST_JOIN]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: return
// CHECK: push [[CONT]]
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: sload
// CHECK-NEXT: push 1
// CHECK-NEXT: sload
// CHECK-NEXT: dup 1
// CHECK-NEXT: dup 3
// CHECK-NEXT: push 170
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: log3
// CHECK-NEXT: swap 1
// CHECK-NEXT: swap 2
// CHECK-NEXT: push 0
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi [[JOIN:bb[0-9]+]], [[OTHER:bb[0-9]+]]
// CHECK: [[OTHER]]:
// CHECK-NEXT: swap 1
// CHECK-NEXT: jump [[JOIN]]
// CHECK: [[JOIN]]:
// CHECK-NEXT: swap 1
// CHECK-NEXT: push [[SECOND:[0-9]+]]
// CHECK-NEXT: mstore
// CHECK-NEXT: push [[BASE:[0-9]+]]
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: swap 1
// CHECK-NEXT: jump
// Zero-length log data avoids adding a preceding source-memory writer.
contract ResidentBoundaryControls {
    uint256 private first = 17;
    uint256 private second = 34;

    function missing(uint256 tag) external returns (uint256 result) {
        assembly {
            let x := sload(0)
            log3(0, 0, tag, 170, x)
            result := x
        }
    }

    function firstOpcode(bool select) external returns (uint256 result) {
        assembly {
            let x := sload(0)
            switch select
            case 0 { log2(0, 0, 170, x) }
            default { log2(0, 0, 187, x) }
            result := x
        }
    }

    function successor(bool select) external returns (uint256 a, uint256 b, uint256 checksum) {
        (a, b) = emitAndChoose(select);
        assembly { checksum := xor(a, mul(b, 256)) }
    }

    function emitAndChoose(bool select) internal returns (uint256 a, uint256 b) {
        assembly {
            let x := sload(0)
            let y := sload(1)
            log3(0, 0, 170, x, y)
            switch select
            case 0 { a := x b := y }
            default { a := y b := x }
        }
    }
}
