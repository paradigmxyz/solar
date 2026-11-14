//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: msb 0 => 0
//@ run-call: msb 1 => 0
//@ run-call: msb 2 => 1
//@ run-call: msb 3 => 1
//@ run-call: msb 255 => 7
//@ run-call: msb 256 => 8
//@ run-call: msb 0x9234567800000000 => 63
//@ run-call: msb 0xffffffffffffffffffffffffffffffff => 127
//@ run-call: msb 0x100000000000000000000000000000000 => 128
//@ run-call: msb 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: msb 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: log2 0 => 0
//@ run-call: log2 1 => 0
//@ run-call: log2 2 => 1
//@ run-call: log2 3 => 1
//@ run-call: log2 255 => 7
//@ run-call: log2 256 => 8
//@ run-call: log2 0x9234567800000000 => 63
//@ run-call: log2 0xffffffffffffffffffffffffffffffff => 127
//@ run-call: log2 0x100000000000000000000000000000000 => 128
//@ run-call: log2 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: log2 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: fls 0 => 256
//@ run-call: fls 1 => 0
//@ run-call: fls 2 => 1
//@ run-call: fls 3 => 1
//@ run-call: fls 255 => 7
//@ run-call: fls 256 => 8
//@ run-call: fls 0x9234567800000000 => 63
//@ run-call: fls 0xffffffffffffffffffffffffffffffff => 127
//@ run-call: fls 0x100000000000000000000000000000000 => 128
//@ run-call: fls 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: fls 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: countLeadingZeros 0 => 256
//@ run-call: countLeadingZeros 1 => 255
//@ run-call: countLeadingZeros 2 => 254
//@ run-call: countLeadingZeros 3 => 254
//@ run-call: countLeadingZeros 255 => 248
//@ run-call: countLeadingZeros 256 => 247
//@ run-call: countLeadingZeros 0x9234567800000000 => 192
//@ run-call: countLeadingZeros 0xffffffffffffffffffffffffffffffff => 128
//@ run-call: countLeadingZeros 0x100000000000000000000000000000000 => 127
//@ run-call: countLeadingZeros 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: countLeadingZeros 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: leadingZeros 0 => 255
//@ run-call: leadingZeros 1 => 255
//@ run-call: leadingZeros 2 => 254
//@ run-call: leadingZeros 3 => 254
//@ run-call: leadingZeros 255 => 248
//@ run-call: leadingZeros 256 => 247
//@ run-call: leadingZeros 0x9234567800000000 => 192
//@ run-call: leadingZeros 0xffffffffffffffffffffffffffffffff => 128
//@ run-call: leadingZeros 0x100000000000000000000000000000000 => 127
//@ run-call: leadingZeros 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: leadingZeros 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: mathLog2 0 => 0
//@ run-call: mathLog2 1 => 0
//@ run-call: mathLog2 2 => 1
//@ run-call: mathLog2 3 => 1
//@ run-call: mathLog2 255 => 7
//@ run-call: mathLog2 256 => 8
//@ run-call: mathLog2 0x9234567800000000 => 63
//@ run-call: mathLog2 0xffffffffffffffffffffffffffffffff => 127
//@ run-call: mathLog2 0x100000000000000000000000000000000 => 128
//@ run-call: mathLog2 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: mathLog2 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call-fail: mostSignificantBit 0
//@ run-call: mostSignificantBit 1 => 0
//@ run-call: mostSignificantBit 2 => 1
//@ run-call: mostSignificantBit 3 => 1
//@ run-call: mostSignificantBit 255 => 7
//@ run-call: mostSignificantBit 256 => 8
//@ run-call: mostSignificantBit 0x9234567800000000 => 63
//@ run-call: mostSignificantBit 0xffffffffffffffffffffffffffffffff => 127
//@ run-call: mostSignificantBit 0x100000000000000000000000000000000 => 128
//@ run-call: mostSignificantBit 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: mostSignificantBit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255
//@ run-call: zerosAbove 0 => 255
//@ run-call: zerosAbove 1 => 255
//@ run-call: zerosAbove 2 => 254
//@ run-call: zerosAbove 3 => 254
//@ run-call: zerosAbove 255 => 248
//@ run-call: zerosAbove 256 => 247
//@ run-call: zerosAbove 0x9234567800000000 => 192
//@ run-call: zerosAbove 0xffffffffffffffffffffffffffffffff => 128
//@ run-call: zerosAbove 0x100000000000000000000000000000000 => 127
//@ run-call: zerosAbove 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: zerosAbove 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: checkedZeros 0 => 255
//@ run-call: checkedZeros 1 => 255
//@ run-call: checkedZeros 256 => 247
//@ run-call: checkedZeros 0x100000000000000000000000000000000 => 127
//@ run-call: checkedZeros 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: both 0 => 255
//@ run-call: both 1 => 255
//@ run-call: both 2 => 510
//@ run-call: both 3 => 510
//@ run-call: both 255 => 2040
//@ run-call: both 256 => 2295
//@ run-call: both 0x9234567800000000 => 16320
//@ run-call: both 0xffffffffffffffffffffffffffffffff => 32640
//@ run-call: both 0x100000000000000000000000000000000 => 32895
//@ run-call: both 0x8000000000000000000000000000000000000000000000000000000000000000 => 65280
//@ run-call: both 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 65280

// The searches for the highest set bit that PRBMath, Solady, OpenZeppelin and Uniswap ship, each
// written its own way and each differing in what it returns for zero. Every one is a `clz`.
contract LibraryScans {
    // PRBMath's `Common.msb`: the word is shifted down step by step.
    // CHECK-LABEL: fn @msb
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: {{v[0-9]+}} = sub 255, [[ZEROS]]
    // CHECK-NOT: {{ }}gt{{ }}
    // CHECK-NOT: {{ }}shr{{ }}
    // CHECK: returndata
    function msb(uint256 x) external pure returns (uint256 result) {
        assembly ("memory-safe") {
            let factor := shl(7, gt(x, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(6, gt(x, 0xFFFFFFFFFFFFFFFF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(5, gt(x, 0xFFFFFFFF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(4, gt(x, 0xFFFF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(3, gt(x, 0xFF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(2, gt(x, 0xF))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := shl(1, gt(x, 0x3))
            x := shr(factor, x)
            result := or(result, factor)
        }
        assembly ("memory-safe") {
            let factor := gt(x, 0x1)
            result := or(result, factor)
        }
    }

    // Solady's `FixedPointMathLib.log2`: the shift accumulates, and a De Bruijn multiplication
    // indexes a table for the last byte.
    // CHECK-LABEL: fn @log2
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: {{v[0-9]+}} = sub 255, [[ZEROS]]
    // CHECK-NOT: {{ }}byte{{ }}
    // CHECK: returndata
    function log2(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
        }
    }

    // Solady's `LibBit.fls`: 256 for zero, which is one more bit above the index.
    // CHECK-LABEL: fn @fls
    // CHECK: [[ODD:v[0-9]+]] = or [[X:v[0-9]+]], 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: [[INDEX:v[0-9]+]] = sub 255, [[ZEROS]]
    // CHECK-NEXT: [[NONE:v[0-9]+]] = eq [[X]], 0
    // CHECK-NEXT: [[FLAG:v[0-9]+]] = zext i1 [[NONE]] to i256
    // CHECK-NEXT: [[BIT:v[0-9]+]] = shl [[FLAG]], 8
    // CHECK-NEXT: {{v[0-9]+}} = or [[INDEX]], [[BIT]]
    // CHECK-NOT: {{ }}byte{{ }}
    // CHECK: returndata
    function fls(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := or(shl(8, iszero(x)), shl(7, lt(0xffffffffffffffffffffffffffffffff, x)))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
        }
    }

    // Solady's `LibBit.clz`: the table holds the complements, and zero counts 256, which is the
    // opcode as it is.
    // CHECK-LABEL: fn @countLeadingZeros
    // CHECK: [[X:v[0-9]+]] = calldataload 4
    // CHECK-NEXT: {{v[0-9]+}} = clz [[X]]
    // CHECK-NEXT: mstore
    function countLeadingZeros(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := add(xor(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0xf8f9f9faf9fdfafbf9fdfcfdfafbfcfef9fafdfafcfcfbfefafafcfbffffffff)), iszero(x))
        }
    }

    // The start of Solady's `lnWad`: the same count without the correction for zero.
    // CHECK-LABEL: fn @leadingZeros
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: {{v[0-9]+}} = clz [[ODD]]
    // CHECK-NOT: {{ }}byte{{ }}
    // CHECK-NOT: {{ }}xor{{ }}
    // CHECK: returndata
    function leadingZeros(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := xor(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0xf8f9f9faf9fdfafbf9fdfcfdfafbfcfef9fafdfafcfcfbfefafafcfbffffffff))
        }
    }

    function toUint(bool b) internal pure returns (uint256 u) {
        assembly ("memory-safe") {
            u := iszero(iszero(b))
        }
    }

    // OpenZeppelin's `Math.log2`: Solidity operators over a helper call, and a nibble table.
    // CHECK-LABEL: fn @mathLog2
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: {{v[0-9]+}} = sub 255, [[ZEROS]]
    // CHECK-NOT: {{ }}byte{{ }}
    // CHECK: returndata
    function mathLog2(uint256 x) external pure returns (uint256 r) {
        r = toUint(x > 0xffffffffffffffffffffffffffffffff) << 7;
        r |= toUint((x >> r) > 0xffffffffffffffff) << 6;
        r |= toUint((x >> r) > 0xffffffff) << 5;
        r |= toUint((x >> r) > 0xffff) << 4;
        r |= toUint((x >> r) > 0xff) << 3;
        r |= toUint((x >> r) > 0xf) << 2;
        assembly ("memory-safe") {
            r := or(r, byte(shr(r, x), 0x0000010102020202030303030303030300000000000000000000000000000000))
        }
    }

    // Uniswap v3's `BitMath.mostSignificantBit`: branches and checked additions.
    // CHECK-LABEL: fn @mostSignificantBit
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: {{v[0-9]+}} = sub 255, [[ZEROS]]
    // CHECK-NOT: {{ }}select{{ }}
    // CHECK: returndata
    function mostSignificantBit(uint256 x) external pure returns (uint256 r) {
        require(x > 0);
        if (x >= 0x100000000000000000000000000000000) { x >>= 128; r += 128; }
        if (x >= 0x10000000000000000) { x >>= 64; r += 64; }
        if (x >= 0x100000000) { x >>= 32; r += 32; }
        if (x >= 0x10000) { x >>= 16; r += 16; }
        if (x >= 0x100) { x >>= 8; r += 8; }
        if (x >= 0x10) { x >>= 4; r += 4; }
        if (x >= 0x4) { x >>= 2; r += 2; }
        if (x >= 0x2) r += 1;
    }

    // A count taken from the index: the outer value is the scan, so the index is not computed.
    // CHECK-LABEL: fn @zerosAbove
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: {{v[0-9]+}} = clz [[ODD]]
    // CHECK-NOT: {{ }}sub{{ }}
    // CHECK: returndata
    function zerosAbove(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
            r := sub(255, r)
        }
    }

    function index(uint256 x) internal pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
        }
    }

    // A checked subtraction from the index of a helper: the index is at most 255, so the check
    // goes, and the two subtractions cancel.
    // CHECK-LABEL: fn @checkedZeros
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: {{v[0-9]+}} = clz [[ODD]]
    // CHECK-NEXT: mstore
    // CHECK-NOT: revert
    // CHECK: returndata
    function checkedZeros(uint256 x) external pure returns (uint256) {
        return 255 - index(x);
    }

    // The index is read too: the count is the one the index is taken from.
    // CHECK-LABEL: fn @both
    // CHECK: [[ODD:v[0-9]+]] = or {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: [[INDEX:v[0-9]+]] = sub 255, [[ZEROS]]
    // CHECK-NEXT: [[HIGH:v[0-9]+]] = shl [[INDEX]], 8
    // CHECK-NEXT: {{v[0-9]+}} = or [[HIGH]], [[ZEROS]]
    // CHECK-NEXT: mstore
    function both(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
            r := or(r, byte(and(0x1f, shr(shr(r, x), 0x8421084210842108cc6318c6db6d54be)),
                0x0706060506020504060203020504030106050205030304010505030400000000))
            r := or(shl(8, r), sub(255, r))
        }
    }
}
