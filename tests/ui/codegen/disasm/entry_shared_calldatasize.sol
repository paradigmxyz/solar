//@ codegen-matrix: standard runtime
//@[runtime] compile-flags: -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME

// The dispatcher reads `calldatasize` for `receive` and for the short-calldata check, and CSE
// merges the two reads. Stackify rereads it at each use, so the dispatcher stays block-local and
// each selector test is `EQ` plus a jump to its case, not `SUB`, a jump past it, and a `POP`.
// RUNTIME-LABEL: ; === {{.*}}:E (runtime) ===
// RUNTIME: PUSH4 0x0c55699c
// RUNTIME-NEXT: EQ
// RUNTIME: PUSH4 0x4df7e3d0
// RUNTIME-NEXT: EQ
// RUNTIME: PUSH4 0x8c6a0b00
// RUNTIME-NEXT: EQ
contract E {
    uint256 public x;

    receive() external payable {}

    // The selector of `f477()` ends in a zero byte, which makes the dispatcher test the
    // calldata length first.
    function f477() external returns (uint256) {
        return ++x;
    }

    function b() external returns (uint256) {
        return x += 2;
    }

    function c() external returns (uint256) {
        return x += 3;
    }
}
