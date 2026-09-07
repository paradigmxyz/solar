//@ compile-flags: -Zdump=mir
//@ filecheck:

// A library function's selector needs no deploy-time address, so it lowers
// even though `address(L)` does not; see `library_address_unsupported.sol`.
library L {
    function f(uint256 v) external pure returns (uint256) {
        return v;
    }
}

contract C {
    // CHECK-LABEL: fn @sel() [selector=0x41c910e1, pure]
    // CHECK: mstore 128, 0xb3de648b00000000000000000000000000000000000000000000000000000000
    function sel() public pure returns (bytes4) {
        return L.f.selector;
    }
}
