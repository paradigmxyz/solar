//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x341fda35
    // CHECK: calldataload
    // CHECK: add
    // CHECK: calldataload
    // CHECK: dup 1
    // CHECK: push {{[0-9]+}}
    // CHECK: mstore
    // CHECK: push 4
    // CHECK: dup 2
    // CHECK: gt
    // CHECK: push [[TRIM:bb[0-9]+]]
    // CHECK: jumpi
    // CHECK: push {{[0-9]+}}
    // CHECK: mload
    // CHECK: [[TRIM]]:
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
