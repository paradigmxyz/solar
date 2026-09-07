//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:
contract AssemblerBlockDedup {
    // The two constant-one selectors must enter the same complete body.
    // CHECK-LABEL: @module AssemblerBlockDedup_runtime
    // CHECK: callvalue
    // CHECK-NEXT: jumpi [[FAIL:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: push 0xdbe671f
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ONE:bb[0-9]+]], {{bb[0-9]+}}
    function a() public pure returns (uint256) {
        return 1;
    }

    // CHECK: push 0x4df7e3d0
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ONE]], {{bb[0-9]+}}
    function b() public pure returns (uint256) {
        return 1;
    }

    // Both conditional selectors share the complete decoder and failure path.
    // CHECK: push 0x5ce8bda8
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[TWO:bb[0-9]+]], {{bb[0-9]+}}
    function c(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }

    // Complete scratch returns keep the coalesced bodies compact without a
    // transfer to a separate common return block.
    // CHECK: push 0xfeb97429
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[FAIL]], [[TWO]]
    // CHECK: [[TWO]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 36
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[FAIL]], [[DECODE:bb[0-9]+]]
    // CHECK-NEXT: [[DECODE]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 2
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[BOOL:bb[0-9]+]], [[FAIL]]
    // CHECK: [[FAIL]]{{( \[cold\])?}}:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[BOOL]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: jumpi [[FAIL]], [[TWO_RETURN:bb[0-9]+]]
    // CHECK-NEXT: [[TWO_RETURN]]:
    // CHECK-NEXT: push 2
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[ONE]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    function d(bool fail) public pure returns (uint256) {
        if (fail) revert();
        return 2;
    }
}
