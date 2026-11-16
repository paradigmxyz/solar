//@ codegen-matrix: standard
//@ compile-flags: --evm-version prague
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: log2 0 => 0
//@ run-call: log2 1 => 0
//@ run-call: log2 0x9234567800000000 => 63
//@ run-call: log2 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 255

// `CLZ` arrives with Osaka: for an earlier fork the search is the code.
contract BeforeOsaka {
    // CHECK-LABEL: fn @log2
    // CHECK-NOT: clz
    // CHECK: byte
    // CHECK-NOT: clz
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
}
