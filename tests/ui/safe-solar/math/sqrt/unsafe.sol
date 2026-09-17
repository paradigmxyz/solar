//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: root 0 => 0
//@ run-call: root 17 => 4
//@ run-call: root 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 340282366920938463463374607431768211455

// Solady's square root, as shipped: the guess by comparisons and shifts,
// seven Newton steps written out, and a final correction.
// CHECK-LABEL: fn @root
// CHECK: div
contract Unsafe {
    function root(uint256 x) public pure returns (uint256 z) {
        assembly ("memory-safe") {
            z := 181
            let r := shl(7, lt(0xffffffffffffffffffffffffffffffffff, x))
            r := or(r, shl(6, lt(0xffffffffffffffffff, shr(r, x))))
            r := or(r, shl(5, lt(0xffffffffff, shr(r, x))))
            r := or(r, shl(4, lt(0xffffff, shr(r, x))))
            z := shl(shr(1, r), z)
            z := shr(18, mul(z, add(shr(r, x), 65536)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := shr(1, add(z, div(x, z)))
            z := sub(z, lt(div(x, z), z))
        }
    }
}
