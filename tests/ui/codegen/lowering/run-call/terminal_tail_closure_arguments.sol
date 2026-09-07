//@ codegen-matrix: standard ir raw
//@[ir] filecheck: --implicit-check-not=mload
//@[ir] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[raw] filecheck: --implicit-check-not=mload
//@[raw] compile-flags: -Ogas -Zdump=evm-ir-runtime -Zmir-pipeline=lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-evm-shaped
//@ run-call-fail: ordered 0, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000090000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: ordered 1, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: ordered 2, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000009
//@ run-call-fail: reversed 0, 7, 9 => 0x000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: duplicated 1, 7 => 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: ordered 3, 0, 1 => 0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000003

// Different actual orders enter one branching helper, then share a fixed-range leaf.
// Size and raw Gas bypass both argument entries while preserving the ordered payload.
// CHECK-LABEL: @module TailClosureArguments_runtime
// CHECK: dup {{[34]}}
// CHECK: {{push 0[[:space:]]+mstore}}
// CHECK: push 32
// CHECK-NEXT: mstore
// CHECK: push 64
// CHECK-NEXT: mstore
// CHECK: push 96
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
contract TailClosureArguments {
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
            mstore(0, a)
            mstore(32, b)
            mstore(64, tag)
            revert(0, 96)
        }
    }
}
