//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: sort [] => []
//@ run-call: sort [1] => [1]
//@ run-call: sort [4, 3, 2, 1] => [1, 2, 3, 4]
//@ run-call: sort [0, 0, 1, 0] => [0, 0, 0, 1]
//@ run-call: sort [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0] => [0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff]
//@ run-call: sort [5, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5] => [1, 1, 2, 3, 4, 5, 5, 5, 5, 6, 9]
//@ run-call: shiftAt [10, 20, 30, 40], 0 => [10, 20, 30, 40]
//@ run-call: shiftAt [10, 20, 30, 40], 2 => [10, 20, 20, 40]
//@ run-call-fail: shiftAt [10, 20, 30, 40], 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A checked insertion sort reloads `a.length` before every element access
// and tests every index in the source. The length is read once outside the
// loops, the element stores never alias it, and the descending inner index
// stays below the outer counter, which stays below the length: no bounds
// check survives in either loop and the payload is never read twice.
contract CheckedInsertionShift {
    // OPT-LABEL: fn @sort
    // OPT: [[LEN:v[0-9]+]] = mload [[OBJ:v[0-9]+]]
    // OPT-NEXT: jump [[HEADER:bb[0-9]+]]
    // OPT: [[HEADER]]:
    // OPT-NEXT: [[I:v[0-9]+]] = phi
    // OPT-NEXT: {{v[0-9]+}} = lt [[I]], [[LEN]]
    // OPT-NOT: mstore 4, 50
    // OPT: {{v[0-9]+}} = phi [{{bb[0-9]+}}: [[I]]]
    // OPT-NOT: mload [[OBJ]]
    // OPT-NOT: mstore 4, 50
    // OPT: jump [[HEADER]]
    function sort(uint256[] memory a) external pure returns (uint256[] memory) {
        for (uint256 i = 1; i < a.length; ++i) {
            uint256 value = a[i];
            uint256 j = i;
            while (j != 0 && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
        return a;
    }

    // The index arrives from the caller, so its bound is unknown and the
    // first access keeps its check; the panic block stays reachable.
    // OPT-LABEL: fn @shiftAt
    // OPT: mstore 4, 50
    function shiftAt(uint256[] memory a, uint256 j) external pure returns (uint256[] memory) {
        if (j != 0) {
            a[j] = a[j - 1];
        }
        return a;
    }
}
