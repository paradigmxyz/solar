//@compile-flags: -O none --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@filecheck:

library Lib {
    function pair() public pure returns (uint256, uint256) {
        return (4, 5);
    }
}

contract C {
    // Linked-library calls return through DELEGATECALL into the input area.
    // Lowering loads the returned words before tuple extraction.
    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: [[INPUT:v[0-9]+]] = slice_ptr
    // CHECK: delegatecall {{v[0-9]+}}, {{.*}}, [[INPUT]], {{v[0-9]+}}, [[INPUT]], 64
    // CHECK-NOT: returndata_bytes
    // CHECK: [[PAIR:v[0-9]+]] = insert_value {{struct[0-9]+}}, {{v[0-9]+}}, 1
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 1
    function pair() external pure returns (uint256, uint256) {
        return Lib.pair();
    }
}
