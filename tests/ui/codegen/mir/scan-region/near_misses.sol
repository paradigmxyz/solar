//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: wrongEntry 5 => 3
//@ run-call: wrongEntry 4 => 2
//@ run-call: wrongEntry 0x50000000000000000000000000000000000000000 => 163
//@ run-call: wrongEntry 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: offByOne 0xffff => 16
//@ run-call: offByOne 0xfffe => 15
//@ run-call: offByOne 0x10000 => 16
//@ run-call: offByOne 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: stopsEarly 3 => 0
//@ run-call: stopsEarly 4 => 2
//@ run-call: stopsEarly 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 254
//@ run-call: otherZero 0 => 7
//@ run-call: otherZero 1 => 0
//@ run-call: otherZero 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: halves 1 => 0
//@ run-call: halves 0x100000000000000000000000000000000 => 128
//@ run-call: halves 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 128

// Searches that are not the index of the highest set bit for some input. Each agrees with it on
// most inputs, and none becomes a full CLZ index. scan-region still replaces the steps of a
// search that round the index down to a nibble or a byte, which are exact, and keeps the wrong
// table or comparison after them.
contract NearMisses {
    function toUint(bool b) internal pure returns (uint256 u) {
        assembly ("memory-safe") {
            u := iszero(iszero(b))
        }
    }

    // The table's entry for five is three: wrong for every word whose leading bits are 101.
    // CHECK-LABEL: fn @wrongEntry
    // CHECK: clz
    // CHECK: and {{.*}}, 252
    // CHECK: byte
    // CHECK-NOT: clz
    // CHECK: returndata
    function wrongEntry(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
        r |= toUint((x >> r) > 0xffffffffffffffff) << 6;
        r |= toUint((x >> r) > 0xffffffff) << 5;
        r |= toUint((x >> r) > 0xffff) << 4;
        r |= toUint((x >> r) > 0xff) << 3;
        r |= toUint((x >> r) > 0xf) << 2;
        assembly ("memory-safe") {
            r := or(r, byte(shr(r, x), 0x0000010102030202030303030303030300000000000000000000000000000000))
        }
    }

    // One threshold is one too low: wrong for the words whose low half ends at `0xffff`.
    // CHECK-LABEL: fn @offByOne
    // CHECK: clz
    // CHECK: and {{.*}}, 224
    // CHECK: gt {{.*}}, 0xfffe
    // CHECK: byte
    // CHECK-NOT: clz
    // CHECK: returndata
    function offByOne(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
        r |= toUint((x >> r) > 0xffffffffffffffff) << 6;
        r |= toUint((x >> r) > 0xffffffff) << 5;
        r |= toUint((x >> r) > 0xfffe) << 4;
        r |= toUint((x >> r) > 0xff) << 3;
        r |= toUint((x >> r) > 0xf) << 2;
        assembly ("memory-safe") {
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303030300000000000000000000000000000000))
        }
    }

    // The search stops one step short and only finds even indexes: the index rounded down to
    // even, (m >> 1) << 1, which is a CLZ form.
    // CHECK-LABEL: fn @stopsEarly
    // CHECK: clz
    // CHECK: and {{.*}}, 254
    // CHECK-NOT: jumpi
    // CHECK: returndata
    function stopsEarly(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
        r |= toUint((x >> r) > 0xffffffffffffffff) << 6;
        r |= toUint((x >> r) > 0xffffffff) << 5;
        r |= toUint((x >> r) > 0xffff) << 4;
        r |= toUint((x >> r) > 0xff) << 3;
        r |= toUint((x >> r) > 0xf) << 2;
        r |= toUint((x >> r) > 0x3) << 1;
    }

    // The index for every word but zero, for which no form of the opcode returns seven.
    // CHECK-LABEL: fn @otherZero
    // CHECK: clz
    // CHECK: and {{.*}}, 252
    // CHECK: byte
    // CHECK-NOT: clz
    // CHECK: returndata
    function otherZero(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
        r |= toUint((x >> r) > 0xffffffffffffffff) << 6;
        r |= toUint((x >> r) > 0xffffffff) << 5;
        r |= toUint((x >> r) > 0xffff) << 4;
        r |= toUint((x >> r) > 0xff) << 3;
        r |= toUint((x >> r) > 0xf) << 2;
        assembly ("memory-safe") {
            r := or(r, byte(shr(r, x), 0x0700010102020202030303030303030300000000000000000000000000000000))
        }
    }

    // A test of the upper half alone starts a search and is not one.
    // CHECK-LABEL: fn @halves
    // CHECK-NOT: clz
    // CHECK: returndata
    function halves(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
    }
}
