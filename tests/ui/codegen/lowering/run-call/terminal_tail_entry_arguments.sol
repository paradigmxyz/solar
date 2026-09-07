//@ codegen-matrix: standard ir raw
//@[ir] filecheck: --implicit-check-not=mload
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[raw] filecheck: --check-prefixes=CHECK,RAW --implicit-check-not=mload
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: ordered 7, 9 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000009
//@ run-call-fail: reversed 7, 9 => 0x00000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: duplicated 7 => 0x00000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: ordered 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: reversed 0, 1 => 0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: duplicated 0 => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

// CHECK-LABEL: @module TerminalTailEntryArguments_runtime
// CHECK: push 0x4410d3bb
// CHECK: push 0xe77aaaab
// CHECK: push 0xfe67ba36
// CHECK: push 36
// CHECK-NEXT: calldataload
// CHECK: push 4
// CHECK-NEXT: calldataload
// RAW-NEXT: swap 2
// RAW-NEXT: pop
// CHECK: jump [[BODY:bb[0-9]+]]
// CHECK: [[BODY]] [cold]:
// CHECK: push 0
// CHECK-NEXT: mstore
// CHECK: push 32
// CHECK-NEXT: mstore
// CHECK: {{push 64[[:space:]]+push 0[[:space:]]+revert}}
// CHECK: dup 1
// RAW-NEXT: swap 2
// RAW-NEXT: pop
// CHECK: jump [[BODY]]
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK: jump [[BODY]]

contract TerminalTailEntryArguments {
    function ordered(uint256 a, uint256 b) external pure {
        fail(a, b, 999);
    }

    function reversed(uint256 a, uint256 b) external pure {
        fail(b, a, 999);
    }

    function duplicated(uint256 a) external pure {
        fail(a, a, 999);
    }

    // Several call sites retain this leaf; the final formal is deliberately unused.
    function fail(uint256 a, uint256 b, uint256 unused) internal pure {
        assembly {
            mstore(0, a)
            mstore(32, b)
            mstore(64, xor(a, b))
            mstore(96, add(a, b))
            mstore(128, sub(a, b))
            revert(0, 64)
        }
    }
}
