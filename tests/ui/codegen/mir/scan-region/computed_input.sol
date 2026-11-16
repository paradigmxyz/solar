//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: estimate 0, 0 => 1
//@ run-call: estimate 0x100, 0 => 0x10
//@ run-call: estimate 0x80, 0x180 => 0x10
//@ run-call: estimate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0x80000000000000000000000000000000
//@ run-call: byteIndex 0 => 0
//@ run-call: byteIndex 0xfe => 0
//@ run-call: byteIndex 0xff => 8
//@ run-call: byteIndex 0xffffffffffffffffffffffffffffffff => 0x80
//@ run-call: byteIndex 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xf8

// The scanned word is computed in the region's first block and only the search reads it. The
// `clz` that replaces the search reads it too, so its definition stays.
contract ComputedInput {
    // CHECK-DAG: [[XOR:v[0-9]+]] = xor
    // CHECK-DAG: [[XOR_BIT:v[0-9]+]] = or [[XOR]], 1
    // CHECK-DAG: clz [[XOR_BIT]]
    function estimate(uint256 x, uint256 y) external pure returns (uint256 result) {
        uint256 xAux = x ^ y;
        result = 1;
        if (xAux >= 2 ** 128) { xAux >>= 128; result <<= 64; }
        if (xAux >= 2 ** 64) { xAux >>= 64; result <<= 32; }
        if (xAux >= 2 ** 32) { xAux >>= 32; result <<= 16; }
        if (xAux >= 2 ** 16) { xAux >>= 16; result <<= 8; }
        if (xAux >= 2 ** 8) { xAux >>= 8; result <<= 4; }
        if (xAux >= 2 ** 4) { xAux >>= 4; result <<= 2; }
        if (xAux >= 2 ** 2) { result <<= 1; }
    }

    // A whole byte scan of a computed word.
    // CHECK-DAG: [[ADD:v[0-9]+]] = add {{v[0-9]+}}, 1
    // CHECK-DAG: [[ADD_BIT:v[0-9]+]] = or [[ADD]], 1
    // CHECK-DAG: clz [[ADD_BIT]]
    function byteIndex(uint256 x) public pure returns (uint256 r) {
        assembly {
            x := add(x, 1)
            r := shl(7, lt(0xffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffff, shr(r, x))))
            r := or(r, shl(3, lt(0xff, shr(r, x))))
        }
    }
}
