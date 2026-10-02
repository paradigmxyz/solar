//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: sum 0 => 0
//@ run-call: sum 1 => 1
//@ run-call: sum 2 => 5
//@ run-call: sum 3 => 12
//@ run-call: sum 4 => 22
//@ run-call: sum 5 => 35
//@ run-call: sum 1000 => 1499500
//@ run-call: stepped 5 => 0
//@ run-call: stepped 6 => 5
//@ run-call: stepped 8 => 5
//@ run-call: stepped 9 => 43
//@ run-call: stepped 12 => 312
//@ run-call: stepped 15 => 2198
//@ run-call: stepped 100 => 1012392034723593925779857584
//@ run-call: guarded 2, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaac
//@ run-call-fail: guarded 3, 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: guarded 4, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: guarded 3, 0x5555555555555555555555555555555555555555555555555555555555555556 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Gas builds run these loops' bodies twice per test of their bound. The
// overflows that `guarded` reports happen in the second copy, the first copy,
// and the original loop.
// CHECK-LABEL: fn @entry()
// `stepped` starts at five and steps by three; its body comes first in the
// dispatcher, then `guarded` and `sum`, which count from zero.
// CHECK: [[I:v[0-9]+]] = phi [{{bb[0-9]+}}: 5], {{\[}}{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK: [[I_AHEAD:v[0-9]+]] = add [[I]], 3
// CHECK-NEXT: {{v[0-9]+}} = lt [[I_AHEAD]], {{v[0-9]+}}
// CHECK: phi [{{bb[0-9]+}}: [[I]]]
// CHECK: [[J:v[0-9]+]] = phi [{{bb[0-9]+}}: 0], {{\[}}{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK: [[J_AHEAD:v[0-9]+]] = add [[J]], 1
// CHECK-NEXT: {{v[0-9]+}} = lt [[J_AHEAD]], {{v[0-9]+}}
// CHECK: phi [{{bb[0-9]+}}: [[J]]]
// CHECK: [[K:v[0-9]+]] = phi [{{bb[0-9]+}}: 0], {{\[}}{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK: [[K_AHEAD:v[0-9]+]] = add [[K]], 1
// CHECK-NEXT: {{v[0-9]+}} = lt [[K_AHEAD]], {{v[0-9]+}}
// CHECK: phi [{{bb[0-9]+}}: [[K]]]
contract LoopUnroll {
    function sum(uint256 n) external pure returns (uint256 s) {
        for (uint256 i; i < n; ++i) s += i * 3 + 1;
    }

    function stepped(uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 5; i < n; i += 3) {
            unchecked { s = s * 7 + i; }
        }
    }

    function guarded(uint256 n, uint256 x) external pure returns (uint256 s) {
        for (uint256 i; i < n; ++i) s += x;
    }
}
