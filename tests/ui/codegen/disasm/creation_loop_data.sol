//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir
//@[ir] filecheck:
//@ run-call: result => 0x0000000000000000000000000000000000000000000000000000000000000000

// A loop in the constructor copies its four literal words from the data section in one copy,
// as the runtime does, instead of storing each from a push on every iteration.
// CHECK-LABEL: @module LoopData_deployment
// CHECK: [loop]:
// CHECK-NEXT: push 128
// CHECK-NEXT: push_data literal_{{[0-9]+}}
contract LoopData {
    bytes32 public result;

    constructor() {
        bytes32 acc;
        for (uint256 i; i < 40; ++i) {
            assembly {
                mstore(0x00, 0x1111111111111111111111111111111111111111111111111111111111111111)
                mstore(0x20, 0x2222222222222222222222222222222222222222222222222222222222222222)
                mstore(0x40, 0x3333333333333333333333333333333333333333333333333333333333333333)
                mstore(0x60, 0x4444444444444444444444444444444444444444444444444444444444444444)
                acc := xor(acc, keccak256(0x00, 0x80))
            }
        }
        result = acc;
    }
}
