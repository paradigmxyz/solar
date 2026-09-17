//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: fls 0 => 256
//@ run-call: clz 0 => 256
//@ run-call: ffs 0 => 256
//@ run-call: fls 1 => 0
//@ run-call: clz 1 => 255
//@ run-call: ffs 1 => 0
//@ run-call: fls 2 => 1
//@ run-call: clz 2 => 254
//@ run-call: ffs 2 => 1
//@ run-call: fls 3 => 1
//@ run-call: clz 3 => 254
//@ run-call: ffs 3 => 0
//@ run-call: fls 7 => 2
//@ run-call: clz 7 => 253
//@ run-call: ffs 7 => 0
//@ run-call: fls 8 => 3
//@ run-call: clz 8 => 252
//@ run-call: ffs 8 => 3
//@ run-call: fls 255 => 7
//@ run-call: clz 255 => 248
//@ run-call: ffs 255 => 0
//@ run-call: fls 256 => 8
//@ run-call: clz 256 => 247
//@ run-call: ffs 256 => 8
//@ run-call: fls 18446744073709551616 => 64
//@ run-call: clz 18446744073709551616 => 191
//@ run-call: ffs 18446744073709551616 => 64
//@ run-call: fls 340282366920938463463374607431768211455 => 127
//@ run-call: clz 340282366920938463463374607431768211455 => 128
//@ run-call: ffs 340282366920938463463374607431768211455 => 0
//@ run-call: fls 340282366920938463463374607431768211456 => 128
//@ run-call: clz 340282366920938463463374607431768211456 => 127
//@ run-call: ffs 340282366920938463463374607431768211456 => 128
//@ run-call: fls 57896044618658097711785492504343953926634992332820282019728792003956564819973 => 255
//@ run-call: clz 57896044618658097711785492504343953926634992332820282019728792003956564819973 => 0
//@ run-call: ffs 57896044618658097711785492504343953926634992332820282019728792003956564819973 => 0
//@ run-call: fls 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 255
//@ run-call: clz 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: ffs 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0

// The frozen checked bit-search port: eight `if` steps that each shift the
// word and set a result bit. Every step is a pure two-phi diamond, so it
// becomes a scaled shift and a scaled or with no branch; only the zero test
// keeps its jump.
library LibBit {
    function fls(uint256 x) internal pure returns (uint256 r) {
        if (x == 0) return 256;
        if (x >> 128 != 0) {
            x >>= 128;
            r = 128;
        }
        if (x >> 64 != 0) {
            x >>= 64;
            r |= 64;
        }
        if (x >> 32 != 0) {
            x >>= 32;
            r |= 32;
        }
        if (x >> 16 != 0) {
            x >>= 16;
            r |= 16;
        }
        if (x >> 8 != 0) {
            x >>= 8;
            r |= 8;
        }
        if (x >> 4 != 0) {
            x >>= 4;
            r |= 4;
        }
        if (x >> 2 != 0) {
            x >>= 2;
            r |= 2;
        }
        return r | (x >> 1);
    }

    function clz(uint256 x) internal pure returns (uint256 r) {
        return x == 0 ? 256 : 255 - fls(x);
    }

    function ffs(uint256 x) internal pure returns (uint256 r) {
        return x == 0 ? 256 : fls(x & (~x + 1));
    }
}

contract CheckedBitSearch {
    // OPT-LABEL: fn @fls{{[.0-9]*}}(arg0: u256)
    // OPT: jumpi arg0, [[SEARCH:bb[0-9]+]], {{bb[0-9]+}}
    // OPT: [[SEARCH]]:
    // OPT: [[SCALED:v[0-9]+]] = shl 7, {{v[0-9]+}}
    // OPT-NEXT: {{v[0-9]+}} = shr [[SCALED]], arg0
    // OPT-NOT: jumpi
    // OPT: ret {{v[0-9]+}}
    function fls(uint256 x) external pure returns (uint256) {
        return LibBit.fls(x);
    }

    function clz(uint256 x) external pure returns (uint256) {
        return LibBit.clz(x);
    }

    function ffs(uint256 x) external pure returns (uint256) {
        return LibBit.ffs(x);
    }
}
