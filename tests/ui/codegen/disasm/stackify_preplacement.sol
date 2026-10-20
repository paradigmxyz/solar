//@ compile-flags: -Ogas --evm-version osaka -Zdump=disasm-runtime
//@ filecheck:
//@ run-call: chain [3, 4, 5, 6], 11 => 5
//@ run-call: chain [0, 4, 5, 6], 11 => 8

// A word that one instruction computes just before the chain feeding its consumer's top
// operand runs with that chain, after the words below it, when that plan is cheaper than
// copying the word first. Each `mulmod` takes `n` deepest, so the snapshot shows it pushed
// once and consumed in place, with fewer swaps than copying it first and shuffling it under.
// CHECK: ISZERO
// CHECK-NEXT: PUSH1 {{.*}}; bb1
// CHECK-NEXT: JUMPI
// CHECK-NEXT: DUP5
// CHECK-NEXT: DUP2
// CHECK-NEXT: DUP5
// CHECK-NEXT: MULMOD
// CHECK-NEXT: DUP6
// CHECK-NEXT: SWAP2
// CHECK-NEXT: DUP3
// CHECK-NEXT: SWAP2
// CHECK-NEXT: SWAP6
// CHECK-NEXT: MULMOD
// CHECK-NEXT: SWAP2
// CHECK-NEXT: DUP2
// CHECK-NEXT: SWAP4
// CHECK-NEXT: MULMOD
// CHECK-NEXT: ADDMOD
// CHECK-NEXT: ADDMOD
contract StackifyPreplacement {
    function chain(uint256[4] memory x, uint256 n) external pure returns (uint256 r) {
        unchecked {
            uint256 a = x[0];
            uint256 b = x[1];
            uint256 c = x[2];
            uint256 d = x[3];
            r = addmod(mulmod(a, b, n), mulmod(c, d, n), n);
            r = addmod(r, mulmod(a, d, n), n);
        }
    }
}
