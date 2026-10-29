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

// A checked insertion sort still carries its element pointer through the loop. Its length
// loads and checks stay when alias analysis cannot prove that indexed stores miss the header.
contract CheckedInsertionShift {
    // OPT-LABEL: fn @sort
    // OPT: [[OBJ:v[0-9]+]] = icall @decode_calldata_type
    // OPT-NEXT: [[CURSOR:v[0-9]+]] = add [[OBJ]], 64
    // OPT-NEXT: jump [[HEADER:bb[0-9]+]]
    // OPT: [[HEADER]]:
    // OPT-NEXT: [[I:v[0-9]+]] = phi
    // OPT-NEXT: {{v[0-9]+}} = phi [{{bb[0-9]+}}: [[CURSOR]]]
    // OPT-NEXT: [[LEN:v[0-9]+]] = mload [[OBJ]]
    // OPT-NEXT: {{v[0-9]+}} = lt [[I]], [[LEN]]
    // OPT: {{v[0-9]+}} = phi [{{bb[0-9]+}}: [[I]]]
    // OPT: mload [[OBJ]]
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
    // OPT: tail_call @revert_stub
    // OPT-LABEL: fn @revert_stub
    // OPT: mstore 32, 50
    function shiftAt(uint256[] memory a, uint256 j) external pure returns (uint256[] memory) {
        if (j != 0) {
            a[j] = a[j - 1];
        }
        return a;
    }
}
