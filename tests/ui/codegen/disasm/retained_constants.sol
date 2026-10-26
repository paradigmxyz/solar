//@ codegen-matrix: standard runtime
//@[runtime] compile-flags: -Ogas -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: f 3, 5, 0 => 2143
//@ run-call: f 3, 5, 1 => 6293
//@ run-call: f 3, 5, 2 => 323
//@ run-call: f 5373003642731685151011, 308109520888805757326122170, 0 => 1977472449865439805662739045027155680725276699658091484651
//@ run-call: f 5373003642731685151011, 308109520888805757326122170, 1 => 3204951115731685146291303309586376840590263358707394038111
//@ run-call: f 5373003642731685151011, 308109520888805757326122170, 7 => 54115159670168897341511896440058639013322913955702436493364457261

// Both masks are used in several blocks. The stack keeps each of them after its first use
// instead of pushing the 64-bit mask again, or rebuilding the 128-bit one, in every branch.
// RUNTIME-LABEL: // === {{.*}}:C (runtime) ===
// RUNTIME: PUSH8 0xffffffffffffffff
// RUNTIME-NOT: PUSH8
// RUNTIME: NOT
// RUNTIME-NOT: NOT
contract C {
    function f(uint256 x, uint256 y, uint256 k) external pure returns (uint256 r) {
        assembly {
            r := and(x, 0xffffffffffffffff)
            r := add(r, and(y, 0xffffffffffffffffffffffffffffffff))
            switch k
            case 0 {
                r := xor(r, and(mul(r, x), 0xffffffffffffffff))
                r := add(r, and(y, 0xffffffffffffffffffffffffffffffff))
                r := xor(r, and(mul(r, r), 0xffffffffffffffffffffffffffffffff))
            }
            case 1 {
                r := xor(r, and(mul(r, y), 0xffffffffffffffff))
                r := add(r, and(x, 0xffffffffffffffffffffffffffffffff))
                r := xor(r, and(mul(r, r), 0xffffffffffffffffffffffffffffffff))
            }
            default {
                r := mul(r, and(r, 0xffffffffffffffff))
            }
            r := mul(r, and(y, 0xffffffffffffffff))
            r := add(r, and(x, 0xffffffffffffffffffffffffffffffff))
        }
    }
}
