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
    // CHECK: returndata_bytes
    // CHECK: [[PAIR:v[0-9]+]] = abi_decode [u256, u256]
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 1
    function pair() external pure returns (uint256, uint256) {
        return Lib.pair();
    }
}
