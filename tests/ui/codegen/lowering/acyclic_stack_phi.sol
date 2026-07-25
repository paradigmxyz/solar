//@ compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x341fda35
    // CHECK: calldataload
    // CHECK-NEXT: add
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup1
    // CHECK-NEXT: push {{[0-9]+}}
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup2
    // CHECK-NEXT: gt
    // CHECK-NEXT: push [[TRIM:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: jump [[MERGE:bb[0-9]+]]
    // CHECK-NEXT: [[MERGE]]:
    // CHECK-NEXT: dup2
    // CHECK-NEXT: dup1
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
