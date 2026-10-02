//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: toHex 0x => 0x
//@ run-call: toHex 0x0a => 0x3061
//@ run-call: toHex 0x0aff => 0x30616666
//@ run-call: toHex 0x010203 => 0x303130323033
//@ run-call: toHex 0x0001020304 => 0x30303031303230333034
//@ run-call: toHex 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f => 0x30303031303230333034303530363037303830393061306230633064306530663130313131323133313431353136313731383139316131623163316431653166
//@ run-call: toHex 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x303030313032303330343035303630373038303930613062306330643065306631303131313231333134313531363137313831393161316231633164316531663230
//@ run-call: sumWords [] => 0
//@ run-call: sumWords [7] => 7
//@ run-call: sumWords [1, 2, 3] => 6
//@ run-call: sumWords [1, 2, 3, 4, 5, 6, 7] => 28
//@ run-call: stepThree 5, 5 => 0
//@ run-call: stepThree 0, 9 => 9
//@ run-call: stepThree 0, 12 => 18
//@ run-call: stepThree 3, 18 => 45
//@ run-call: stepThree 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd, 3 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd
//@ run-call-fail: stepThree 1, 2; gas=200000

// Assembly loops that run until a counter reaches its bound peel one iteration
// when an odd number remain, then test the bound once per two iterations. A
// counter that wraps keeps its exact count, and one that never reaches its
// bound runs out of gas in every build.
contract PointerLoops {
    // The hex digits of `raw`, written as Solady's `toHexStringNoPrefix` does.
    // CHECK-LABEL: fn @toHex()
    // CHECK: mstore 15, 0x30313233343536373839616263646566
    // CHECK: [[LEFT:v[0-9]+]] = sub {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: [[ODD:v[0-9]+]] = trunc i256 [[LEFT]] to i1
    // CHECK-NEXT: jumpi [[ODD]], {{bb[0-9]+}}, [[HEADER:bb[0-9]+]]
    // CHECK: [[HEADER]]:
    // CHECK-NEXT: phi [{{bb[0-9]+}}: {{v[0-9]+}}], [{{bb[0-9]+}}: {{v[0-9]+}}], [{{bb[0-9]+}}: {{v[0-9]+}}]
    function toHex(bytes memory raw) external pure returns (bytes memory result) {
        assembly {
            let n := mload(raw)
            result := add(mload(0x40), 2)
            mstore(result, add(n, n))
            mstore(0x0f, 0x30313233343536373839616263646566)
            let o := add(result, 0x20)
            let end := add(raw, n)
            for {} iszero(eq(raw, end)) {} {
                raw := add(raw, 1)
                mstore8(add(o, 1), mload(and(mload(raw), 15)))
                mstore8(o, mload(and(shr(4, mload(raw)), 15)))
                o := add(o, 2)
            }
            mstore(o, 0)
            mstore(0x40, add(o, 0x20))
        }
    }

    // Words step by 32, so the parity of the remaining count is bit 5 of the distance.
    // CHECK-LABEL: fn @sumWords()
    // CHECK: calldatacopy
    // CHECK: [[LEFT:v[0-9]+]] = sub {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: [[COUNT:v[0-9]+]] = shr 5, [[LEFT]]
    // CHECK-NEXT: trunc i256 [[COUNT]] to i1
    function sumWords(uint256[] memory values) external pure returns (uint256 s) {
        assembly {
            let p := add(values, 0x20)
            let end := add(p, shl(5, mload(values)))
            for {} iszero(eq(p, end)) { p := add(p, 0x20) } { s := add(s, mload(p)) }
        }
    }

    // CHECK-LABEL: fn @stepThree()
    // CHECK: [[LEFT:v[0-9]+]] = sub arg1, arg0
    // CHECK-NEXT: trunc i256 [[LEFT]] to i1
    // CHECK: [[I:v[0-9]+]] = phi [{{bb[0-9]+}}: arg0], [{{bb[0-9]+}}: [[NEXT:v[0-9]+]]], [{{bb[0-9]+}}: {{v[0-9]+}}]
    // CHECK: [[SECOND:v[0-9]+]] = add [[I]], 3
    // CHECK: [[NEXT]] = add [[SECOND]], 3
    function stepThree(uint256 start, uint256 end) external pure returns (uint256 sum) {
        assembly {
            for { let i := start } iszero(eq(i, end)) { i := add(i, 3) } { sum := add(sum, i) }
        }
    }
}
