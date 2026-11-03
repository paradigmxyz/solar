//@ codegen-matrix: standard runtime
//@[runtime] compile-flags: -Osize -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: f 0x => 0
//@ run-call: f 0xaa => 170
//@ run-call: f 0x00ff => 0
//@ run-call: f 0xff00 => 255

// `length > 0` is a nonzero test, so the branch jumps on the length itself: a bare `ISZERO`
// follows the argument checks, with no `LT` against a pushed zero. The else arm's `0` is the word
// pushed before the branch, and the jump reaches the join directly, as at `-Ogas`.
// RUNTIME-LABEL: ; === {{.*}}:R2 (runtime) ===
// RUNTIME: SWAP3
// RUNTIME-NEXT: OR
// RUNTIME-NEXT: PUSH1 {{.*}}
// RUNTIME-NEXT: JUMPI
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: SWAP1
// RUNTIME-NEXT: ISZERO
// RUNTIME-NEXT: PUSH1 {{0x[0-9a-f]+}} ; bb[[JOIN:[0-9]+]]
// RUNTIME-NEXT: JUMPI
// RUNTIME-NEXT: SWAP1
// RUNTIME-NEXT: CALLDATALOAD
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: BYTE
// RUNTIME-NEXT: ; bb[[JOIN]]
// RUNTIME-NEXT: JUMPDEST
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: MSTORE
contract R2 {
    function f(bytes calldata data) external pure returns (uint256) {
        return data.length > 0 ? uint8(data[0]) : 0;
    }
}
