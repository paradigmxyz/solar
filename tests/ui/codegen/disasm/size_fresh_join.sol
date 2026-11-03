//@ codegen-matrix: standard runtime
//@[runtime] compile-flags: -Osize -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: f => 0
//@ run-call: g 0 => 0
//@ run-call: g 77345497107738552411838431392522000948863935069627095510731433067785723314176 => 171

// The else arm's `0` is the constant that `mstore 0` and `return 0` in the join also read. The
// planner pushes it again at each use, so the phi result takes over the word pushed before the
// branch instead of keeping it beside a second copy. The branch pushes the `0` once, the side
// edge reaches the join directly, and no `PUSH0 PUSH1 join JUMP` edge block follows.
// RUNTIME-LABEL: ; === {{.*}}:C (runtime) ===
// RUNTIME: JUMPDEST
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: PUSH1 0x04
// RUNTIME-NEXT: CALLDATASIZE
// RUNTIME-NEXT: EQ
// RUNTIME-NEXT: PUSH1 {{0x[0-9a-f]+}} ; bb[[JOIN:[0-9]+]]
// RUNTIME-NEXT: JUMPI
// RUNTIME: ; bb[[JOIN]]
// RUNTIME-NEXT: JUMPDEST
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: MSTORE
// RUNTIME-NEXT: PUSH1 0x20
// RUNTIME-NEXT: PUSH0
// RUNTIME-NEXT: RETURN
// RUNTIME-NOT: PUSH1 {{0x[0-9a-f]+}} ; bb[[JOIN]]
contract C {
    function f() external pure returns (uint256) {
        bytes calldata data;
        assembly {
            data.offset := 4
            data.length := sub(calldatasize(), 4)
        }
        return data.length > 0 ? uint8(data[0]) : 0;
    }

    function g(uint256) external pure returns (uint256) {
        bytes calldata data;
        assembly {
            data.offset := 4
            data.length := sub(calldatasize(), 4)
        }
        return data.length > 0 ? uint8(data[0]) : 0;
    }
}
