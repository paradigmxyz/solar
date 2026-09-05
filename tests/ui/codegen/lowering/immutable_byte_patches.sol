//@ compile-flags: -O size -Zdump=evm-ir
//@ filecheck:

contract ImmutableBytePatches {
    uint8 immutable unsignedValue;
    int8 immutable signedValue;
    bytes1 immutable fixedBytesValue;

    constructor(uint8 unsignedValue_, int8 signedValue_, bytes1 fixedBytesValue_) {
        unsignedValue = unsignedValue_;
        signedValue = signedValue_;
        fixedBytesValue = fixedBytesValue_;
    }

    // CHECK-LABEL: @module ImmutableBytePatches_deployment
    // CHECK: codecopy
    // CHECK: codecopy
    // Match each complete patch independently: its order is not observable.
    // The two integer patches store their low byte; bytes1 extracts its high byte.
    // CHECK-DAG: mload{{[[:space:]]+push [0-9]+[[:space:]]+mstore8}}
    // CHECK-DAG: mload{{[[:space:]]+push [0-9]+[[:space:]]+mstore8}}
    // CHECK-DAG: mload{{[[:space:]]+push 0[[:space:]]+byte[[:space:]]+push [0-9]+[[:space:]]+mstore8}}
    // CHECK: return
    function read() external view returns (uint8, int8, bytes1) {
        return (unsignedValue, signedValue, fixedBytesValue);
    }
}
