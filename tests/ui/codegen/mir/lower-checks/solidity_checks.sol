//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=lower-checks -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: req 1 => 1
//@[run] run-call-fail: req 0 => 0x
//@[run] run-call: reqMsg 1 => 1
//@[run] run-call-fail: reqMsg 0 => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000047a65726f00000000000000000000000000000000000000000000000000000000
//@[run] run-call: asrt 1 => 1
//@[run] run-call-fail: asrt 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001
//@[run] run-call: custom 1 => 1
//@[run] run-call-fail: custom 0 => 0xa2f431300000000000000000000000000000000000000000000000000000000000000007
//@[run] run-call: plain 1 => 2
// Each `require`, `assert`, and custom revert splits its block and branches to a payload block
// that stages the revert data in scratch memory, or calls a shared helper for an error string.
// A function without checks keeps its straight-line body.

contract Checks {
    error Bad(uint256 code);

    // A bare `require` reverts without data.
    // CHECK-LABEL: fn @req(
    // CHECK-NOT: icall require
    // CHECK: jumpi
    // CHECK: revert 0, 0
    function req(uint256 x) public pure returns (uint256) {
        require(x != 0);
        return x;
    }

    // A short message calls the shared `revert_error` helper instead of staging the payload.
    // CHECK-LABEL: fn @reqMsg(
    // CHECK-NOT: icall require
    // CHECK: icall @revert_error, 4, 0x7a65726f
    function reqMsg(uint256 x) public pure returns (uint256) {
        require(x != 0, "zero");
        return x;
    }

    // `assert` stages `Panic(1)` in scratch memory.
    // CHECK-LABEL: fn @asrt(
    // CHECK: mstore 32, 1
    // CHECK-NEXT: mstore 0, 0x4e487b71
    // CHECK-NEXT: revert 28, 36
    function asrt(uint256 x) public pure returns (uint256) {
        assert(x != 0);
        return x;
    }

    // A custom error stages its selector and argument in scratch memory.
    // CHECK-LABEL: fn @custom(
    // CHECK: mstore 32, {{v[0-9]+}}
    // CHECK-NEXT: mstore 0, 0xa2f43130
    // CHECK-NEXT: revert 28, 36
    function custom(uint256 x) public pure returns (uint256) {
        if (x == 0) revert Bad(x + 7);
        return x;
    }

    // CHECK-LABEL: fn @plain(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: checked_add uint256, arg0, 1
    // CHECK-NEXT: ret
    function plain(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
