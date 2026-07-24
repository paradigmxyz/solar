//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

// `abi.encodePacked(...)` may include a `bytes`/`string` calldata argument,
// which is packed as its raw data (no length prefix, no padding). The calldata
// is copied into a `[len][data]` memory buffer and then packed like any other
// dynamic bytes value. Used by nitro-contracts MockRollupEventInbox. The packed
// bytes (hashed here) are verified equal to solc 0.8.30 separately.

contract P {
    // CHECK: push 0x21bd63cb
    // CHECK: eq
    // CHECK-NEXT: push [[H_BODY:bb[0-9]+]]
    // CHECK: push 0xf1245422
    // CHECK: eq
    // CHECK-NEXT: push [[H2_BODY:bb[0-9]+]]
    // CHECK: [[H_BODY]]:
    // CHECK: calldatacopy
    // CHECK: mcopy
    // CHECK: jump [[DONE:bb[0-9]+]]
    function h(bytes calldata a, uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, x));
    }

    // CHECK: [[DONE]]:
    // CHECK: keccak256
    // CHECK: [[H2_BODY]]:
    // CHECK: calldatacopy
    // CHECK: push 0x707265
    // CHECK: mcopy
    // CHECK: jump [[DONE]]
    function h2(bytes calldata a, address b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("pre", a, b));
    }
}
