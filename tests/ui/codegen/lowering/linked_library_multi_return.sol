//@compile-flags: -Zcodegen -O none --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=mir
//@filecheck:

library Lib {
    function pair() public pure returns (uint256, uint256) {
        return (4, 5);
    }
}

contract C {
    // Linked-library calls return through DELEGATECALL. Until MIR has
    // first-class multi-result calls, lowering must publish the return buffer
    // used by tuple extraction.
    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: delegatecall
    // CHECK: mstore 32, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = mload 32
    // CHECK: mload {{v[0-9]+}}
    function pair() external pure returns (uint256, uint256) {
        return Lib.pair();
    }
}
