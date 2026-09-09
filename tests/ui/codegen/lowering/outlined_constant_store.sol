//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Osize -Zevm-ir-pipeline=outline -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: first 128 => 7
//@ run-call: second 128 => 9
//@ run-call: first 255 => 7
//@ run-call: second 160 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
//@ run-call: first 0 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
//@ run-call: second 0 => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef

// A fixed store is a short but expensive sequence. The later potentially aliasing write prevents
// the load from being forwarded, so outlining must preserve both the store and the caller's input.
contract OutlinedConstantStore {
    // CHECK-LABEL: @module OutlinedConstantStore_runtime
    // CHECK: calldatasize
    // CHECK: {{^bb[0-9]+:}}
    // CHECK-NEXT: push [[RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[STORE:bb[0-9]+]]
    // CHECK: jump [[STORE]]
    // CHECK: [[STORE]]:
    // CHECK-NEXT: push 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump
    // CHECK: [[RET]] [continuation]:
    function first(uint256 pointer) external pure returns (uint256 value) {
        assembly {
            mstore(128, 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
            mstore(and(pointer, 128), 7)
            value := mload(128)
        }
    }

    function second(uint256 pointer) external pure returns (uint256 value) {
        assembly {
            mstore(128, 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
            mstore(and(pointer, 160), 9)
            value := mload(128)
        }
    }
}
