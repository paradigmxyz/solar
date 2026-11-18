//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: first 1 => 0
//@ run-call: first 0x100000000000000000000000000000000 => 128
//@ run-call: second 0xffff => 15
//@ run-call: third 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: missFirst 3 => 1
//@ run-call: missFirst 4 => 2
//@ run-call: missSecond 8 => 3
//@ run-call: missSecond 14 => 2
//@ run-call: missSecond 0xffff => 15

// Functions that hold the same search each prove it. Each copy of a scan is rewritten, and each
// copy of a near miss keeps its table, though the pass still rounds the near miss's index down to
// a nibble with CLZ, which is exact. The copies end up identical, so the optimized dispatcher
// sends each selector to one shared block.
// CHECK-LABEL: fn @entry
// CHECK: switch {{.*}}0xb3f62f5 => [[SCAN:bb[0-9]+]], 0x691d13c2 => [[MISS:bb[0-9]+]], 0x7896dfe3 => [[SCAN]], 0xca7b9b97 => [[MISS]], 0xf1680ae6 => [[SCAN]]]
// CHECK: [[MISS]]:
// CHECK: clz
// CHECK: and {{.*}}, 252
// CHECK: byte
// CHECK: returndata
// CHECK: [[SCAN]]:
// CHECK: clz
// CHECK-NOT: byte
// CHECK: returndata
contract SharedProofs {
    function first(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303030300000000000000000000000000000000))
        }
    }

    function second(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303030300000000000000000000000000000000))
        }
    }

    function third(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303030300000000000000000000000000000000))
        }
    }

    // The table's entry for fourteen is two: wrong for every word whose leading bits are 1110,
    // which only the runs that fix the nibble below the highest set bit try, and they fail it.
    function missFirst(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303020300000000000000000000000000000000))
        }
    }

    // The same near miss again: it fails here too.
    function missSecond(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303020300000000000000000000000000000000))
        }
    }
}
