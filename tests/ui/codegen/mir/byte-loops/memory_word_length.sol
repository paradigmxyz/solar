//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: copy 0, 0, 0 => 0
//@ run-call: copy 0, 0, 1 => 11
//@ run-call: copy 1, 0, 1 => 22
//@ run-call: copy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0
//@ run-call: copy 0x800000000000000000000000000000000000000000000000000000000000000, 0, 1 => 11
//@ run-call: copy 0, 0x800000000000000000000000000000000000000000000000000000000000000, 1 => 11
//@ run-call: copy 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 2 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: copy 0, 0, 0x800000000000000000000000000000000000000000000000000000000000000; gas=200000
//@ run-call-fail: copy 0, 0, 0x800000000000000000000000000000000000000000000000000000000000001; gas=200000
//@ run-call-fail: copy 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract Test {
    // Forged lengths can admit counts whose byte size truncates to zero or one word. The
    // original loop cannot finish these copies, so they must not take the MCOPY path. Memory
    // indexing itself is modular: a large offset can still name an ordinary element.
    // CHECK-LABEL: fn @copy(
    // CHECK: [[OVERFLOW:v[0-9]+]] = shr arg2, 251
    // CHECK-NEXT: [[REASONS:v[0-9]+]] = or {{v[0-9]+}}, [[OVERFLOW]]
    // CHECK-NEXT: [[FALLBACK:v[0-9]+]] = ne [[REASONS]], 0
    // CHECK-NEXT: jumpi [[FALLBACK]]
    // CHECK: mcopy
    // CHECK: phi
    function copy(uint256 sOff, uint256 dOff, uint256 count) external pure returns (uint256) {
        uint256[] memory src = new uint256[](2);
        uint256[] memory dst = new uint256[](2);
        src[0] = 11;
        src[1] = 22;
        assembly {
            mstore(src, not(0))
            mstore(dst, not(0))
        }
        for (uint256 k; k < count; ++k) {
            dst[dOff + k] = src[sOff + k];
        }
        return dst[0];
    }
}
