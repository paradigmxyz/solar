//@ compile-flags: -O gas -Zdump=mir-final
//@ filecheck:

// A returned calldata struct or fixed array encodes straight from calldata into the return
// buffer, as `abi.encode` does: no memory object is allocated and no copy runs through memory.
contract CalldataReturnDirect {
    struct S {
        uint256 value;
        bytes tail;
    }

    // CHECK-LABEL: fn @ret()
    // CHECK-NOT: mstore 64
    // CHECK-NOT: mload 64
    // CHECK-NOT: mcopy
    // CHECK: calldatacopy
    // CHECK-NOT: calldatacopy
    // CHECK-NOT: mcopy
    // CHECK: returndata
    function ret(S calldata s) external pure returns (S calldata) {
        return s;
    }

    // CHECK-LABEL: fn @reta()
    // CHECK-NOT: mstore 64
    // CHECK-NOT: mload
    // CHECK: calldatacopy 128, 4, 64
    // CHECK-NEXT: returndata 128, 64
    function reta(uint256[2] calldata a) external pure returns (uint256[2] calldata) {
        return a;
    }
}
