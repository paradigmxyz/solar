//@ codegen-matrix: standard ir irsize
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[irsize] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[irsize] filecheck:
//@ run-call: mulmodPair 7, 9, 10 => 4
//@ run-call: addmodPair 7, 9, 10 => 7
//@ run-call: mulmodPair 0x8000000000000000000000000000000000000000000000000000000000003039, 0x100000000000000000000000000000000000000000000000063, 1000003 => 790015
//@ run-call: addmodPair 0x8000000000000000000000000000000000000000000000000000000000003039, 0x100000000000000000000000000000000000000000000000063, 1000003 => 155131

// `addmod(a, b, n)` and `mulmod(a, b, n)` are commutative in `a` and `b` only. The planner tries
// both orders of that pair and keeps the modulus in place, so the copies of `n` and `a` are pushed
// first and the computed `b` goes last, on top, where it needs no swap.
// CHECK-LABEL: @module CommutativePairOrder_runtime
// CHECK: dup 2
// CHECK-NEXT: dup 2
// CHECK-NEXT: push 2
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: addmod
// CHECK-NEXT: addmod
// CHECK: dup 2
// CHECK-NEXT: dup 2
// CHECK-NEXT: push 2
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: mulmod
// CHECK-NEXT: mulmod
contract CommutativePairOrder {
    function mulmodPair(uint256, uint256, uint256) external pure returns (uint256 r) {
        assembly {
            let n := calldataload(68)
            let a := add(calldataload(4), 1)
            let b := add(calldataload(36), 2)
            let t := mulmod(a, b, n)
            r := mulmod(t, a, n)
            mstore(0, b)
            mstore(32, n)
        }
    }

    function addmodPair(uint256, uint256, uint256) external pure returns (uint256 r) {
        assembly {
            let n := calldataload(68)
            let a := add(calldataload(4), 1)
            let b := add(calldataload(36), 2)
            let t := addmod(a, b, n)
            r := addmod(t, a, n)
            mstore(0, b)
            mstore(32, n)
        }
    }
}
