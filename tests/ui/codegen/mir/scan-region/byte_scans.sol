//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: byteIndex 0 => 0
//@ run-call: byteIndex 1 => 0
//@ run-call: byteIndex 0xf => 0
//@ run-call: byteIndex 0x10 => 0
//@ run-call: byteIndex 0xff => 0
//@ run-call: byteIndex 0x100 => 8
//@ run-call: byteIndex 0x1234 => 8
//@ run-call: byteIndex 0xffffffffffffffff => 0x38
//@ run-call: byteIndex 0x10000000000000000 => 0x40
//@ run-call: byteIndex 0xffffffffffffffffffffffffffffffff => 0x78
//@ run-call: byteIndex 0x100000000000000000000000000000000 => 0x80
//@ run-call: byteIndex 0x8000000000000000000000000000000000000000000000000000000000000007 => 0xf8
//@ run-call: byteIndex 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xf8
//@ run-call: nibbleIndex 0 => 0
//@ run-call: nibbleIndex 1 => 0
//@ run-call: nibbleIndex 0xf => 0
//@ run-call: nibbleIndex 0x10 => 4
//@ run-call: nibbleIndex 0xff => 4
//@ run-call: nibbleIndex 0x100 => 8
//@ run-call: nibbleIndex 0x1234 => 0xc
//@ run-call: nibbleIndex 0xffffffffffffffff => 0x3c
//@ run-call: nibbleIndex 0x10000000000000000 => 0x40
//@ run-call: nibbleIndex 0xffffffffffffffffffffffffffffffff => 0x7c
//@ run-call: nibbleIndex 0x100000000000000000000000000000000 => 0x80
//@ run-call: nibbleIndex 0x8000000000000000000000000000000000000000000000000000000000000007 => 0xfc
//@ run-call: nibbleIndex 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfc
//@ run-call: byteLength 0 => 0x20
//@ run-call: byteLength 1 => 0x1f
//@ run-call: byteLength 0xf => 0x1f
//@ run-call: byteLength 0x10 => 0x1f
//@ run-call: byteLength 0xff => 0x1f
//@ run-call: byteLength 0x100 => 0x1e
//@ run-call: byteLength 0x1234 => 0x1e
//@ run-call: byteLength 0xffffffffffffffff => 0x18
//@ run-call: byteLength 0x10000000000000000 => 0x17
//@ run-call: byteLength 0xffffffffffffffffffffffffffffffff => 0x10
//@ run-call: byteLength 0x100000000000000000000000000000000 => 0xf
//@ run-call: byteLength 0x8000000000000000000000000000000000000000000000000000000000000007 => 0
//@ run-call: byteLength 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
// Solady scans the highest set bit down to a byte or a nibble and keeps the index rounded down,
// `(m >> k) << k`. The whole scan becomes `and(255 - clz(x | 1), 255 >> k << k)`, also when
// the scan is the only part of its block the pass can run.
contract ByteScans {
    // CHECK-LABEL: fn @byteIndex(
    // CHECK: clz
    // CHECK: and {{.*}}, 248
    // CHECK-NOT: jumpi
    // CHECK: returndata
    function byteIndex(uint256 x) public pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
        }
    }

    // CHECK-LABEL: fn @nibbleIndex(
    // CHECK: clz
    // CHECK: and {{.*}}, 252
    // CHECK-NOT: jumpi
    // CHECK: returndata
    function nibbleIndex(uint256 x) public pure returns (uint256 r) {
        assembly {
            r := shl(7, iszero(iszero(shr(128, x))))
            r := or(r, shl(6, iszero(iszero(shr(64, shr(r, x))))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, shl(2, lt(0xf, shr(r, x))))
        }
    }

    // The scan stops at a load, as in Solady's `LibString.eqs`.
    // CHECK-LABEL: fn @byteLength(
    // CHECK: clz
    // CHECK-NOT: jumpi
    // CHECK: mload
    // CHECK: returndata
    function byteLength(uint256 x) public pure returns (uint256 n) {
        assembly {
            let r := shl(7, iszero(iszero(shr(128, x))))
            r := or(r, shl(6, iszero(iszero(shr(64, shr(r, x))))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            n := add(mload(0x60), add(iszero(x), xor(31, shr(3, r))))
        }
    }
}
