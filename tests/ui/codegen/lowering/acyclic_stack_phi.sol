//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x341fda35
    // CHECK: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 224
    // CHECK-NEXT: mload
    // CHECK-NEXT: jump [[MERGE:bb[0-9]+]]
    // CHECK-NEXT: [[MERGE]]:
    // CHECK-NEXT: dup 1
    // CHECK: {{bb[0-9]+}} [cold]:
    // CHECK: [[TRIM:bb[0-9]+]]:
    // CHECK-NEXT: push 4
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
