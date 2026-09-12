//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
// CHECK-LABEL: @module StackWords_runtime
// CHECK: xor
// CHECK-NEXT: dup 1
// CHECK-NEXT: dup 3
// CHECK-NEXT: sub
// CHECK: {{^ *}}or{{$}}
// CHECK-NEXT: dup 2
// CHECK-NEXT: dup 2
// CHECK-NEXT: sub
// CHECK: and
// CHECK-NEXT: dup 2
// CHECK-NEXT: dup 2
// CHECK-NEXT: add
//@ run-call: sum 9, 4 => 0, 13, 13
//@ run-call: sum 7, 7 => 7, 7, 14
//@ run-call: xor 9, 4 => 0, 13, 13
//@ run-call: xor 7, 7 => 7, 7, 0
//@ run-call: merged 9, 4 => 0, 13, 13
//@ run-call: merged 7, 7 => 7, 0, 7
//@ run-call: masked 9, 4 => 13, 13, 0
//@ run-call: masked 7, 7 => 7, 0, 7
//@ run-call: sum 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
contract StackWords {
    function sum(uint256 x, uint256 y) public pure returns (uint256 common, uint256 either, uint256 total) {
        unchecked { common = x & y; either = x | y; total = x + y; }
    }
    function xor(uint256 x, uint256 y) public pure returns (uint256 common, uint256 either, uint256 total) {
        common = x & y;
        either = x | y;
        total = x ^ y;
    }
    function merged(uint256 x, uint256 y) public pure returns (uint256 common, uint256 different, uint256 total) {
        common = x & y;
        different = x ^ y;
        total = x | y;
    }
    function masked(uint256 x, uint256 y) public pure returns (uint256 either, uint256 different, uint256 total) {
        either = x | y;
        different = x ^ y;
        total = x & y;
    }
}
