//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module AcyclicStackPhi_runtime
    // CHECK: push 0x341fda35
    // CHECK-NEXT: sub
    // CHECK-NEXT: push [[REVERT:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[REVERT]] [cold]:
    // CHECK: {{bb[0-9]+}}:
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: pop
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
