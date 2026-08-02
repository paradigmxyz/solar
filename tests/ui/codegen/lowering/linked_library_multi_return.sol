//@compile-flags: -Zcodegen -O none --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@filecheck:

library Lib {
    function pair() public pure returns (uint256, uint256) {
        return (4, 5);
    }
}

contract C {
    // Linked-library calls return through DELEGATECALL. Lowering keeps the
    // returned words in a typed fixed-array object before tuple extraction.
    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: alloc memoryfixedarray<2, 1>
    // CHECK: delegatecall
    // CHECK: mstore 32
    // CHECK: memory_object_load_element memoryfixedarray<2, 1>
    // CHECK: mload 32
    function pair() external pure returns (uint256, uint256) {
        return Lib.pair();
    }
}
