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

// Versioning guards a fresh array's length against wrapping stores before reusing it. `lanes`
// runs at most three times, so its guard folds and it stays rolled even at the runs that copy
// loop bodies.
contract LoopArrayFill {
    function fill(uint256 n) external pure returns (uint256[] memory ids) {
        ids = new uint256[](n);
        for (uint256 i = 0; i < n; ++i) {
            ids[i] = i * i;
        }
    }

    // CHECK-LABEL: fn @lanes
    // CHECK: [[COUNT:v[0-9]+]] = phi [bb0: 0], [bb5: 1], [bb6: {{v[0-9]+}}]
    // CHECK: jump [[HEADER:bb[0-9]+]]
    // CHECK-NEXT: [[HEADER]]:
    // CHECK: = lt {{v[0-9]+}}, [[COUNT]]
    // CHECK-NEXT: jumpi {{v[0-9]+}}, [[BODY:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: [[BODY]]:
    // CHECK-NOT: mload
    // CHECK-NOT: jumpi
    // CHECK: mstore
    // CHECK: jump [[HEADER]]
    // CHECK-LABEL: fn @revert_stub
    // CHECK: mstore 32, 50
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
