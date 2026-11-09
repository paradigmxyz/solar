//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: ffs 0 => 256
//@ run-call: ffs 1 => 0
//@ run-call: ffs 2 => 1
//@ run-call: ffs 3 => 0
//@ run-call: ffs 8 => 3
//@ run-call: ffs 0xff00 => 8
//@ run-call: ffs 0x100000000000000000000000000000000 => 128
//@ run-call: ffs 0x8000000000000000000000000000000000000000000000010000000000000000 => 64
//@ run-call: ffs 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: ffs 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call-fail: leastSignificantBit 0
//@ run-call: leastSignificantBit 1 => 0
//@ run-call: leastSignificantBit 2 => 1
//@ run-call: leastSignificantBit 3 => 0
//@ run-call: leastSignificantBit 8 => 3
//@ run-call: leastSignificantBit 0xff00 => 8
//@ run-call: leastSignificantBit 0x100000000000000000000000000000000 => 128
//@ run-call: leastSignificantBit 0x8000000000000000000000000000000000000000000000010000000000000000 => 64
//@ run-call: leastSignificantBit 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: leastSignificantBit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: wrongEntry 32 => 4
//@ run-call: wrongEntry 16 => 4
//@ run-call: wrongEntry 0 => 256
//@ run-call: wrongEntry 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
//@ run-call: whole 1 => 0
//@ run-call: whole 0x100000000 => 32
//@ run-call: whole 3 => 96

// The lowest set bit: `x & -x` has at most one bit set, and a De Bruijn multiplication maps each
// of its 256 values to its index. The index of the one bit is the index of the highest.
contract LowestBits {
    // Solady's `LibBit.ffs`: 256 for zero.
    // CHECK-LABEL: fn @ffs
    // CHECK: [[LOW:v[0-9]+]] = and
    // CHECK-NEXT: [[ODD:v[0-9]+]] = or [[LOW]], 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: [[INDEX:v[0-9]+]] = sub 255, [[ZEROS]]
    // CHECK-NEXT: [[NONE:v[0-9]+]] = eq [[LOW]], 0
    // CHECK-NEXT: [[FLAG:v[0-9]+]] = zext i1 [[NONE]] to i256
    // CHECK-NEXT: [[BIT:v[0-9]+]] = shl [[FLAG]], 8
    // CHECK-NEXT: {{v[0-9]+}} = or [[INDEX]], [[BIT]]
    // CHECK-NOT: {{ }}mul{{ }}
    // CHECK-NOT: {{ }}div{{ }}
    // CHECK: returndata
    function ffs(uint256 x) external pure returns (uint256 r) {
        assembly {
            x := and(x, add(not(x), 1))
            r := shl(5, shr(252, shl(shl(2, shr(250, mul(x,
                0xb6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff))),
                0x8040405543005266443200005020610674053026020000107506200176117077)))
            r := or(r, byte(and(div(0xd76453e0, shr(r, x)), 0x1f),
                0x001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160405))
        }
    }

    // Uniswap v4's `BitMath.leastSignificantBit`.
    // CHECK-LABEL: fn @leastSignificantBit
    // CHECK: [[LOW:v[0-9]+]] = and
    // CHECK-NEXT: [[ODD:v[0-9]+]] = or [[LOW]], 1
    // CHECK-NEXT: [[ZEROS:v[0-9]+]] = clz [[ODD]]
    // CHECK-NEXT: {{v[0-9]+}} = sub 255, [[ZEROS]]
    // CHECK-NOT: {{ }}mul{{ }}
    // CHECK-NOT: {{ }}div{{ }}
    // CHECK: returndata
    function leastSignificantBit(uint256 x) external pure returns (uint8 r) {
        require(x > 0);
        assembly ("memory-safe") {
            x := and(x, sub(0, x))
            r := shl(5, shr(252, shl(shl(2, shr(250, mul(x,
                0xb6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff))),
                0x8040405543005266443200005020610674053026020000107506200176117077)))
            r := or(r, byte(and(div(0xd76453e0, shr(r, x)), 0x1f),
                0x001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160405))
        }
    }

    // One entry of the table is off by one: wrong for eight of the 256 bits.
    // CHECK-LABEL: fn @wrongEntry
    // CHECK-NOT: clz
    // CHECK: {{ }}div{{ }}
    // CHECK-NOT: clz
    // CHECK: returndata
    function wrongEntry(uint256 x) external pure returns (uint256 r) {
        assembly {
            x := and(x, sub(0, x))
            r := shl(5, shr(252, shl(shl(2, shr(250, mul(x,
                0xb6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff))),
                0x8040405543005266443200005020610674053026020000107506200176117077)))
            r := or(r, byte(and(div(0xd76453e0, shr(r, x)), 0x1f),
                0x001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160404))
        }
    }

    // The same lookup of a whole word is an index only where the word has one bit.
    // CHECK-LABEL: fn @whole
    // CHECK-NOT: clz
    // CHECK: {{ }}div{{ }}
    // CHECK-NOT: clz
    // CHECK: returndata
    function whole(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := shl(5, shr(252, shl(shl(2, shr(250, mul(x,
                0xb6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff))),
                0x8040405543005266443200005020610674053026020000107506200176117077)))
            r := or(r, byte(and(div(0xd76453e0, shr(r, x)), 0x1f),
                0x001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160405))
        }
    }
}
