//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck:
//@ run-call: fact 0 => 1
//@ run-call: fact 5 => 120
//@ run-call: fact 57 => 40526919504877216755680601905432322134980384796226602145184481280000000000000
//@ run-call-fail: fact 58 => Panic(0x11)
//@ run-call: total 0 => 0
//@ run-call: total 100 => 5050

// Recursive activations keep their words on the stack below the return address.
// A call whose result the caller returns unchanged reuses the caller's return
// address and jumps to the callee entry without pushing a new one.
contract Recursion {
    function fact(uint256 n) external pure returns (uint256) {
        return factorial(n);
    }

    function total(uint256 n) external pure returns (uint256) {
        return sum(n, 0);
    }

    // CHECK-LABEL: @module Recursion_runtime
    // CHECK: push 4{{$}}
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push [[RET:bb[0-9]+]]
    // CHECK-NEXT: jump [[FACT:bb[0-9]+]]
    // CHECK-NEXT: [[FACT]]:
    // CHECK-NOT: mstore
    // CHECK: sub
    // CHECK-NEXT: push [[CONT:bb[0-9]+]]
    // CHECK-NEXT: jump [[FACT]]
    function factorial(uint256 n) internal pure returns (uint256) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }

    // CHECK: push 4{{$}}
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push [[RET]]
    // CHECK-NEXT: jump [[SUM:bb[0-9]+]]
    // CHECK-NEXT: [[SUM]]:
    // CHECK: sub
    // CHECK-NOT: push bb
    // CHECK: jump [[SUM]]
    // CHECK: [[CONT]] [continuation]:
    // CHECK-NOT: mload
    // CHECK: mul
    function sum(uint256 n, uint256 acc) internal pure returns (uint256) {
        if (n == 0) return acc;
        unchecked {
            return sum(n - 1, acc + n);
        }
    }
}
