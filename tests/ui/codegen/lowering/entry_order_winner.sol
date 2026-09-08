//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: combine [0, 0, 1], 1, 1, 1 => 0, 0, 1
//@ run-call: combine [1, 2, 1], 3, 4, 1 => 115792089210356248762697446949407573530086143415290314195533631308867097853939, 16, 2
//@ run-call: combine [1, 2, 1], 1, 2, 1 => 115792089210356248762697446949407573530086143415290314195533631308867097853919, 115792089210356248762697446949407573530086143415290314195533631308867097853823, 4
//@ run-call: combine [0, 0, 0], 0, 0, 0 => 0, 0, 0

// Modular arithmetic with two live-value joins and a repeated wide literal.
// A schedule regression test, not a curve-membership or signature verifier.
contract InternalModularEntryWinner {
    function combine(uint256[3] memory point, uint256 x, uint256 y, uint256 z)
        external pure returns (uint256, uint256, uint256)
    {
        return mix(point, x, y, z);
    }

    // Both modular branches preserve the already selected incoming order.
    // The complete old entry winner must precede the optional materialized trial.
    // CHECK-LABEL: @module InternalModularEntryWinner_runtime
    // CHECK: and
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: iszero
    // CHECK-NEXT: jumpi [[ADD:bb[0-9]+]], [[TEST:bb[0-9]+]]
    // CHECK-NEXT: [[TEST]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[DEFAULT:bb[0-9]+]], [[DOUBLE:bb[0-9]+]]
    // CHECK-NEXT: [[DOUBLE]]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: pop
    // CHECK-NEXT: pop
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: swap 3
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: push [[MOD:0x[0-9a-f]+]]
    // CHECK-NEXT: not
    // CHECK: dup 1
    // CHECK-NEXT: dup 6
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: mulmod
    // CHECK: [[ADD]]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 4
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 6
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 6
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: swap 5
    // CHECK-NEXT: swap 3
    // CHECK-NEXT: swap 4
    // CHECK-NEXT: exchange 2, 3
    // CHECK-NEXT: push [[MOD]]
    // CHECK-NEXT: not
    // CHECK: dup 1
    // CHECK-NEXT: dup 8
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: mulmod
    function mix(uint256[3] memory point, uint256 x, uint256 y, uint256 z)
        internal pure returns (uint256 rx, uint256 ry, uint256 rz)
    {
        assembly {
            let p := 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff
            let z1 := mload(add(point, 64))
            let zz1 := mulmod(z1, z1, p)
            let zz2 := mulmod(z, z, p)
            let s1 := mulmod(mload(add(point, 32)), mulmod(zz2, z, p), p)
            let r := addmod(mulmod(y, mulmod(zz1, z1, p), p), sub(p, s1), p)
            let u1 := mulmod(mload(point), zz2, p)
            let h := addmod(mulmod(x, zz1, p), sub(p, u1), p)
            switch and(iszero(r), iszero(h))
            case 0 {
                let hh := mulmod(h, h, p)
                let v := mulmod(u1, hh, p)
                let hhh := mulmod(h, hh, p)
                rx := addmod(addmod(mulmod(r, r, p), sub(p, hhh), p), sub(p, mulmod(2, v, p)), p)
                ry := addmod(mulmod(r, addmod(v, sub(p, rx), p), p), sub(p, mulmod(s1, hhh, p)), p)
                rz := mulmod(h, mulmod(z1, z, p), p)
            }
            case 1 {
                let yy := mulmod(y, y, p)
                let m := addmod(mulmod(3, mulmod(x, x, p), p), mulmod(sub(p, 3), mulmod(zz2, zz2, p), p), p)
                let s := mulmod(4, mulmod(x, yy, p), p)
                rx := addmod(mulmod(m, m, p), sub(p, mulmod(2, s, p)), p)
                let neg := sub(p, mulmod(8, mulmod(yy, yy, p), p))
                ry := addmod(mulmod(m, addmod(s, sub(p, rx), p), p), neg, p)
                rz := mulmod(2, mulmod(y, z, p), p)
            }
        }
    }
}
