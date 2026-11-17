//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: afterScan 0x400000000000000000000000000000000, 0 => 0x82
//@ run-call-fail: afterScan 0x400000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: between 0x400000000000000000000000000000000, 0 => 0x82
//@ run-call-fail: between 0x400000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: inArm 0x400000000000000000000000000000000, 0 => 0x82
//@ run-call: inArm 0xff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 7
//@ run-call-fail: inArm 0x400000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: checkedIndex 0 => 0
//@ run-call: checkedIndex 0x400000000000000000000000000000000 => 0x82
//@ run-call: checkedIndex 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff
//@ run-call: armSub 4 => 253
//@ run-call-fail: armSub 0x100000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: armSub 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A checked operation reverts for some operands, so a run stops at it unless it knows the
// operands: the region never removes a check of the unknown `y`, whether its result is unused
// after the scan, between two steps or inside an arm. The scan before an unused check still
// becomes `clz`, and so does a scan whose steps add with checks, as a run knows their operands.
// A check whose operands a run knows but that fails for some classes keeps the region.
contract CheckedSteps {
    // CHECK-LABEL: fn @afterScan(
    // CHECK: clz
    // CHECK: ret
    function afterScan(uint256 x, uint256 y) external pure returns (uint256 r) {
        unchecked {
            if (x >= 0x100000000000000000000000000000000) { x >>= 128; r += 128; }
            if (x >= 0x10000000000000000) { x >>= 64; r += 64; }
            if (x >= 0x100000000) { x >>= 32; r += 32; }
            if (x >= 0x10000) { x >>= 16; r += 16; }
            if (x >= 0x100) { x >>= 8; r += 8; }
            if (x >= 0x10) { x >>= 4; r += 4; }
            if (x >= 0x4) { x >>= 2; r += 2; }
            if (x >= 0x2) r += 1;
        }
        y + 1;
    }

    function between(uint256 x, uint256 y) external pure returns (uint256 r) {
        unchecked {
            if (x >= 0x100000000000000000000000000000000) { x >>= 128; r += 128; }
        }
        y + 1;
        unchecked {
            if (x >= 0x10000000000000000) { x >>= 64; r += 64; }
            if (x >= 0x100000000) { x >>= 32; r += 32; }
            if (x >= 0x10000) { x >>= 16; r += 16; }
            if (x >= 0x100) { x >>= 8; r += 8; }
            if (x >= 0x10) { x >>= 4; r += 4; }
            if (x >= 0x4) { x >>= 2; r += 2; }
            if (x >= 0x2) r += 1;
        }
    }

    function inArm(uint256 x, uint256 y) external pure returns (uint256 r) {
        if (x >= 0x100000000000000000000000000000000) { x >>= 128; r |= 128; y + 1; }
        if (x >= 0x10000000000000000) { x >>= 64; r += 64; }
        if (x >= 0x100000000) { x >>= 32; r += 32; }
        if (x >= 0x10000) { x >>= 16; r += 16; }
        if (x >= 0x100) { x >>= 8; r += 8; }
        if (x >= 0x10) { x >>= 4; r += 4; }
        if (x >= 0x4) { x >>= 2; r += 2; }
        if (x >= 0x2) r += 1;
    }

    // CHECK-LABEL: fn @checkedIndex(
    // CHECK: clz
    // CHECK-NOT: jumpi
    // CHECK: ret
    function checkedIndex(uint256 x) external pure returns (uint256 r) {
        if (x >= 0x100000000000000000000000000000000) { x >>= 128; r += 128; }
        if (x >= 0x10000000000000000) { x >>= 64; r += 64; }
        if (x >= 0x100000000) { x >>= 32; r += 32; }
        if (x >= 0x10000) { x >>= 16; r += 16; }
        if (x >= 0x100) { x >>= 8; r += 8; }
        if (x >= 0x10) { x >>= 4; r += 4; }
        if (x >= 0x4) { x >>= 2; r += 2; }
        if (x >= 0x2) r += 1;
    }

    // CHECK-LABEL: fn @armSub(
    // CHECK-NOT: clz
    // CHECK: ret
    function armSub(uint256 x) external pure returns (uint256 r) {
        r = 255;
        if (x >= 0x100000000000000000000000000000000) { x >>= 128; r -= 128; }
        if (x >= 0x10000000000000000) { x >>= 64; r -= 64; }
        if (x >= 0x100000000) { x >>= 32; r -= 32; r - 64; }
        if (x >= 0x10000) { x >>= 16; r -= 16; }
        if (x >= 0x100) { x >>= 8; r -= 8; }
        if (x >= 0x10) { x >>= 4; r -= 4; }
        if (x >= 0x4) { x >>= 2; r -= 2; }
        if (x >= 0x2) r -= 1;
    }
}
