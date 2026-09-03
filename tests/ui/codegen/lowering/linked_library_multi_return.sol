//@compile-flags: -O none --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@filecheck:

library Lib {
    function pair() public pure returns (uint256, uint256) {
        return (4, 5);
    }
}

contract C {
    // Linked-library calls return through DELEGATECALL. Lowering decodes the
    // returned words before tuple extraction.
    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: delegatecall
    // CHECK: returndatasize
    // CHECK: mload
    // CHECK: mload
    function pair() external pure returns (uint256, uint256) {
        return Lib.pair();
    }
}
