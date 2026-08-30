//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x341fda35
    // CHECK: calldataload
    // CHECK: add
    // CHECK: calldataload
    // CHECK-NOT: mstore
    // CHECK: push 4
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: gt
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push [[KEEP:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NOT: mload
    // CHECK: jump
    // CHECK: [[KEEP]]:
    // CHECK-NOT: mload
    // CHECK: jump
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
