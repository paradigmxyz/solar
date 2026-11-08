//@ codegen-matrix: standard disasm
//@[disasm] compile-flags: -Ogas -Zdump=disasm-runtime
//@[disasm] filecheck:
//@ run-call: test true, 1, 1 => true
//@ run-call: test false, 1, 1 => true
//@ run-call: test true, 1, 2 => true
//@ run-call: test true, 0x10000000000000000000000000000000000000000, 0 => true
//@ run-call: test false, 0x10000000000000000000000000000000000000000, 0 => true

// Both operand orders of the first `xor` cost the same, and each leaves the two live words
// below the result in a different order. The second `xor` and the comparison after it take
// those words in one order with two fewer swaps, so the planner keeps that order of the first
// `xor`, not the one the MIR writes.
// CHECK: AND
// CHECK-NEXT: SWAP3
// CHECK-NEXT: SWAP1
// CHECK-NEXT: SWAP4
// CHECK-NEXT: XOR
// CHECK-NEXT: PUSH1 0x60
// CHECK-NEXT: SHL
// CHECK-NEXT: SWAP1
// CHECK-NEXT: SWAP3
// CHECK-NEXT: GT
// CHECK-NEXT: EQ
contract StackifyTieLookahead {
    function check(bool success, uint256 signer, uint256 recovered) public pure returns (bool equal) {
        uint256 rds = success ? 0x20 : 0x00;
        bool expected = rds == 0x20 && address(uint160(signer)) == address(uint160(recovered));
        bool optimized;
        assembly {
            optimized := gt(rds, shl(96, xor(signer, recovered)))
        }
        equal = optimized == expected;
    }

    function test(bool success, uint256 signer, uint256 recovered) public pure returns (bool) {
        return check(success, signer, recovered);
    }
}
