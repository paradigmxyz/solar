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
                mstore(0x00, 0x283c9d3e74f799f263598270aadbda723a9eeb23d5e4736cea9c38c6feb4f63f)
                mstore(0x20, 0x07f42d55eccdbc74a6801b505beb897f8ec15d48479d0f0aa4847987616f7613)
                mstore(0x40, 0xcd8b0f07cace797ee8515fb70e78bfa6fc880084d2793c57a3f5f960f99f6703)
                mstore(0x60, 0x9ad26835e2dd5aa0cd526200b033d594fe4834536c4b42dc30955fafdba966dd)
                acc := xor(acc, keccak256(0x00, 0x80))
            }
        }
        result = acc;
    }
}
