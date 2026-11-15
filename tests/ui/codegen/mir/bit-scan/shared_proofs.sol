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
//@ run-call: missSecond 8 => 2
//@ run-call: missSecond 0xffff => 15

// Functions that hold the same search share its proof. Each copy of a scan is rewritten, and
// each copy of a near miss keeps its code, whichever function's proof ran first.
contract SharedProofs {
    // CHECK-LABEL: fn @first
    // CHECK: clz
    // CHECK-NOT: byte
    // CHECK: returndata
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

    // CHECK-LABEL: fn @second
    // CHECK: clz
    // CHECK-NOT: byte
    // CHECK: returndata
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

    // CHECK-LABEL: fn @third
    // CHECK: clz
    // CHECK-NOT: byte
    // CHECK: returndata
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

    // The table's entry for eight is two: wrong for every word whose leading bits are 1000.
    // CHECK-LABEL: fn @missFirst
    // CHECK-NOT: clz
    // CHECK: byte
    // CHECK-NOT: clz
    // CHECK: returndata
    function missFirst(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202020303030303030300000000000000000000000000000000))
        }
    }

    // The same near miss again: it fails its proof here too.
    // CHECK-LABEL: fn @missSecond
    // CHECK-NOT: clz
    // CHECK: byte
    // CHECK-NOT: clz
    // CHECK: returndata
    function missSecond(uint256 x) external pure returns (uint256 r) {
        assembly ("memory-safe") {
            r := shl(7, gt(x, 0xffffffffffffffffffffffffffffffff))
            r := or(r, shl(6, gt(shr(r, x), 0xffffffffffffffff)))
            r := or(r, shl(5, gt(shr(r, x), 0xffffffff)))
            r := or(r, shl(4, gt(shr(r, x), 0xffff)))
            r := or(r, shl(3, gt(shr(r, x), 0xff)))
            r := or(r, shl(2, gt(shr(r, x), 0xf)))
            r := or(r, byte(shr(r, x), 0x0000010102020202020303030303030300000000000000000000000000000000))
        }
    }
}
