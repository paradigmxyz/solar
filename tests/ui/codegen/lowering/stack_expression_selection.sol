//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
// CHECK-LABEL: @module StackExpressionSelection_runtime
// The subtraction and XOR routes share the increment/zero-test tail.
// CHECK: {{^ *}}jump [[SUB:bb[0-9]+]]{{$}}
// CHECK-NEXT: [[SUB]]:
// CHECK-NEXT: mload
// CHECK-NEXT: sub
// CHECK-NEXT: jump [[TAIL:bb[0-9]+]]
// CHECK-NEXT: [[TAIL]]:
// CHECK-NEXT: push 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: add
// CHECK-NEXT: dup 2
// CHECK-NEXT: iszero
// sharedXor(uint256,uint256).
// CHECK: push 0x1a49b55c
// CHECK: xor
// CHECK-NEXT: jump [[TAIL]]
//@ run-call: difference 9, 4 => 5, false
//@ run-call: difference 7, 7 => 0, true
//@ run-call: difference 0, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, false
//@ run-call: reverseDifference 9, 4 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, false
//@ run-call: xorDifference 9, 4 => 13, false
//@ run-call: xorDifference 7, 7 => 0, true
//@ run-call: loop 3 => 3
//@ run-call: sharedDifference 9, 4 => 5, 6, false
//@ run-call: sharedDifference 7, 7 => 0, 1, true
//@ run-call: sharedXor 9, 4 => 13, 14, false
//@ run-call: sharedXor 7, 7 => 0, 1, true
//@ run-call: sharedReverse 9, 4 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc, false
contract StackExpressionSelection {
    function sharedXor(uint256 a, uint256 b) public pure returns (uint256 d, uint256 next, bool equal) {
        unchecked { d = a ^ b; next = d + 1; }
        equal = a == b;
    }
    function sharedReverse(uint256 a, uint256 b) public pure returns (uint256 d, uint256 next, bool equal) {
        unchecked { d = b - a; next = d + 1; }
        equal = a == b;
    }

    function sharedDifference(uint256 a, uint256 b) public pure returns (uint256 d, uint256 next, bool equal) {
        unchecked { d = a - b; next = d + 1; }
        equal = a == b;
    }

    function difference(uint256 a, uint256 b) public pure returns (uint256 d, bool equal) {
        unchecked { d = a - b; }
        equal = a == b;
    }
    function reverseDifference(uint256 a, uint256 b) public pure returns (uint256 d, bool equal) {
        unchecked { d = b - a; }
        equal = a == b;
    }
    function xorDifference(uint256 a, uint256 b) public pure returns (uint256 d, bool equal) {
        d = a ^ b;
        equal = a == b;
    }
    function loop(uint256 n) public pure returns (uint256 result) {
        uint256 previous;
        for (uint256 i; i < n; ++i) {
            // A carried difference describes the previous iteration.
            if (i == previous) ++result;
            previous = i + 1;
        }
    }
}
