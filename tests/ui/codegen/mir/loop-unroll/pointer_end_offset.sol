//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: sumWords [] => 0
//@ run-call: sumWords [5] => 5
//@ run-call: sumWords [1, 2, 3] => 6
//@ run-call: sumWords [1, 2, 3, 4] => 10
//@ run-call: sumPastEnd [] => 0
//@ run-call: sumPastEnd [1, 2, 3] => 6

// The end pointer `(a + len * 32) + 32` and the start pointer `a + 32` carry the same offset,
// so the parity test reads the low bit of the length without subtracting the pointers.
contract PointerEndOffset {
    // CHECK-LABEL: fn @sumWords()
    // CHECK: shl {{v[0-9]+}}, 5
    // CHECK-NEXT: {{v[0-9]+}} = add {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: [[END:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK-NEXT: [[BIT:v[0-9]+]] = and {{v[0-9]+}}, 1
    // CHECK-NEXT: [[ODD:v[0-9]+]] = ne [[BIT]], 0
    // CHECK-NEXT: jumpi [[ODD]], {{bb[0-9]+}}, {{bb[0-9]+}}
    // CHECK: ne {{v[0-9]+}}, [[END]]
    function sumWords(uint256[] memory a) external pure returns (uint256 s) {
        assembly {
            let p := add(a, 32)
            let end := add(add(a, shl(5, mload(a))), 32)
            for {} iszero(eq(p, end)) { p := add(p, 32) } { s := add(s, mload(p)) }
        }
    }

    // Different offsets leave the distance to subtract.
    // CHECK-LABEL: fn @sumPastEnd()
    // CHECK: shl {{v[0-9]+}}, 5
    // CHECK-NEXT: {{v[0-9]+}} = add {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: [[PAST:v[0-9]+]] = add {{v[0-9]+}}, 64
    // CHECK-NEXT: [[DISTANCE:v[0-9]+]] = sub [[PAST]], {{v[0-9]+}}
    function sumPastEnd(uint256[] memory a) external pure returns (uint256 s) {
        assembly {
            let p := add(a, 32)
            let end := add(add(a, shl(5, mload(a))), 64)
            for {} iszero(eq(p, end)) { p := add(p, 32) } { s := add(s, mload(p)) }
        }
    }
}
