//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: fill 0 => []
//@ run-call: fill 3 => [0, 1, 4]
//@ run-call: fill 5 => [0, 1, 4, 9, 16]
//@ run-call: lanes 0 => []
//@ run-call: lanes 0x0000000700000005 => [5, 7]
//@ run-call: lanes 0x000000090000000700000005 => [5, 7, 9]
//@ run-call: lanes 0x0000000b000000090000000700000005 => [5, 7, 9]

// A loop filling a fresh array reads the array's length in every bounds check.
// Only the preheader writes it, so `load-pre` forwards the stored length into
// the body and `check-elim` proves each index below it from the loop's own
// test, leaving no array-bounds panic. `lanes` runs at most three times, so it
// stays rolled even at the runs that copy loop bodies.
contract LoopArrayFill {
    function fill(uint256 n) external pure returns (uint256[] memory ids) {
        ids = new uint256[](n);
        for (uint256 i = 0; i < n; ++i) {
            ids[i] = i * i;
        }
    }

    // CHECK-LABEL: fn @lanes
    // CHECK: [[COUNT:v[0-9]+]] = phi [bb0: 0], [bb5: 1], [bb6: {{v[0-9]+}}]
    // CHECK: = lt {{v[0-9]+}}, [[COUNT]]
    // CHECK-NEXT: jumpi
    // CHECK-NOT: jumpi
    // CHECK-LABEL: fn @revert_stub
    // CHECK-NOT: mstore 32, 50
    function lanes(uint256 packed) external pure returns (uint256[] memory ids) {
        uint256 count = _count(packed);
        ids = new uint256[](count);
        for (uint256 i = 0; i < count; ++i) {
            ids[i] = (packed >> (i * 32)) & type(uint32).max;
        }
    }

    function _count(uint256 packed) private pure returns (uint256) {
        if (packed & type(uint32).max == 0) return 0;
        if ((packed >> 32) & type(uint32).max == 0) return 1;
        if ((packed >> 64) & type(uint32).max == 0) return 2;
        return 3;
    }
}
