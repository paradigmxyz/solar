//@ codegen-matrix: standard ir raw
//@[ir] filecheck:
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[raw] filecheck:
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: ordered 0, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: ordered 1, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: ordered 2, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000009
//@ run-call-fail: reversed 0, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: duplicated 1, 7 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: ordered 3, 0, 1 => 0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000003

// The leaf explicitly defines all returned bytes, giving a portable exact oracle.
// Its revert range overlaps the ancestor argument homes, not just its own frame.
// Retain ancestor stores even when the leaf edge can bypass its disjoint argument entry.
// CHECK-LABEL: @module TailClosureAncestorRange_runtime
// CHECK: push 192
// CHECK-NEXT: mstore
// CHECK-NEXT: push 224
// CHECK-NEXT: mstore
// CHECK-NEXT: push 256
// CHECK-NEXT: mstore
// CHECK: {{push 192[[:space:]]+mload}}
// CHECK-NEXT: push 224
// CHECK-NEXT: mload
// CHECK-NEXT: push 256
// CHECK-NEXT: mload
// CHECK: push 96
// CHECK-NEXT: push 192
// CHECK-NEXT: revert
contract TailClosureAncestorRange {
    function ordered(uint256 mode, uint256 a, uint256 b) external pure {
        route(mode, a, b); revert();
    }
    function reversed(uint256 mode, uint256 a, uint256 b) external pure {
        route(mode, b, a); revert();
    }
    function duplicated(uint256 mode, uint256 a) external pure {
        route(mode, a, a); revert();
    }
    function route(uint256 mode, uint256 a, uint256 b) internal pure {
        if (mode == 0) finish(a, b, mode);
        if (mode == 1) finish(b, a, mode);
        if (mode == 2) finish(a, a, b);
        finish(a, b, mode);
        revert();
    }
    function finish(uint256 a, uint256 b, uint256 tag) internal pure {
        assembly {
            mstore(192, a)
            mstore(224, b)
            mstore(256, tag)
            revert(192, 96)
        }
    }
}
