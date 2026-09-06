//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // The selector miss rejects. Both trim branches return the selected length
    // through the same internal return buffer and continuation.
    // CHECK-LABEL: @module AcyclicStackPhi_runtime
    // CHECK: push 0x341fda35
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[BODY:bb[0-9]+]]
    // CHECK-NEXT: [[BODY]]:
    // CHECK: jumpi [[REJECT]], [[DECODE:bb[0-9]+]]
    // CHECK-NEXT: [[DECODE]]:
    // CHECK: jumpi [[REJECT]], [[TRIM:bb[0-9]+]]
    // CHECK-NEXT: [[TRIM]]:
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push [[RETURN:bb[0-9]+]]
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[SLICE:bb[0-9]+]], [[UNCHANGED:bb[0-9]+]]
    // CHECK-NEXT: [[UNCHANGED]]:
    // CHECK-NEXT: push [[#LENGTH_HOME:]]
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[#LENGTH_HOME - 32]]
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mstore
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump
    // CHECK-NEXT: [[RETURN]]:
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: mload
    // CHECK: return
    // CHECK-NEXT: [[REJECT]]{{( \[cold\])?}}:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK-NEXT: [[SLICE]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: sub
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push [[#LENGTH_HOME]]
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push [[#LENGTH_HOME - 32]]
    // CHECK-NEXT: push 32
    // CHECK-NEXT: mstore
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
