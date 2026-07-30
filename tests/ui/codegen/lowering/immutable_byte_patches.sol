//@ compile-flags: -Zcodegen -O size -Zdump=evm-ir
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

    // CHECK-LABEL: @module deployment
    // CHECK: codecopy
    // CHECK: codecopy
    // CHECK: mload
    // CHECK-NEXT: push
    // CHECK-NEXT: mstore8
    // CHECK: mload
    // CHECK-NEXT: push
    // CHECK-NEXT: mstore8
    // CHECK: mload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: byte
    // CHECK-NEXT: push
    // CHECK-NEXT: mstore8
    function read() external view returns (uint8, int8, bytes1) {
        return (unsignedValue, signedValue, fixedBytesValue);
    }
}
