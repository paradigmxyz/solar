//@ codegen-matrix: standard hot
//@[hot] compile-flags: -Ogas --optimize-runs 9999999 -Zdump=disasm-runtime
//@[hot] filecheck:
//@ run-call: tag 1 => 0xc00000000000000000000001
//@ run-call: low 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0 => 0x123456789abcdef0123456789abcdef0

// From 10,000 optimizer runs, a gas build prices a constant over the full runs, so these masks
// stay literals rather than the shorter recipes of the default 200 runs,
// `PUSH1 0xc0 PUSH1 0x58 SHL` and `PUSH0 NOT PUSH1 0x80 SHR`.
contract HotLiterals {
    // CHECK: PUSH12 0xc00000000000000000000000
    // CHECK-NEXT: PUSH1 0x04
    // CHECK-NEXT: CALLDATALOAD
    // CHECK-NEXT: OR
    function tag(uint256 x) external pure returns (uint256) {
        return x | 0xc00000000000000000000000;
    }

    // CHECK: PUSH16 0xffffffffffffffffffffffffffffffff
    // CHECK-NEXT: PUSH1 0x04
    // CHECK-NEXT: CALLDATALOAD
    // CHECK-NEXT: AND
    function low(uint256 x) external pure returns (uint256) {
        return x & type(uint128).max;
    }
}
