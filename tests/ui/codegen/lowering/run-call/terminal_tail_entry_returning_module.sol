//@ codegen-matrix: standard ir
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=mir,evm-ir-runtime
//@ run-call: sum 3 => 6
//@ run-call: sum 7 => 28
//@ run-call: checked 6 => 6
//@ run-call-fail: checked 0 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000b6c69746572616c206d7367000000000000000000000000000000000000000000

// A returning internal call elsewhere in this module keeps the ordinary shared
// revert entry and its compact shifted literals. The loop must remain outlined.
// CHECK-LABEL: @module TerminalTailEntryReturningModule{{$}}
// CHECK-LABEL: fn @sum
// CHECK: icall @folded, 1,
// CHECK-LABEL: fn @folded
// CHECK: {{^    ret v[0-9]+$}}
// CHECK-LABEL: @module TerminalTailEntryReturningModule_runtime
// CHECK: push 0x6c69746572616c206d7367
// CHECK-NEXT: push 168
// CHECK-NEXT: shl
// CHECK-NEXT: push 11
// CHECK-NEXT: push 224
// CHECK-NEXT: mstore
// CHECK-NEXT: push 256
// CHECK-NEXT: mstore
// CHECK-NEXT: jump [[STUB:bb[0-9]+]]
// CHECK: [[STUB]] [cold]:
// CHECK-NEXT: push 224
// CHECK-NEXT: mload
// CHECK-NEXT: push 256
// CHECK-NEXT: mload
// CHECK: revert
// CHECK: push {{bb[0-9]+}}
// CHECK-NEXT: push 7
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: and
// CHECK-NEXT: push 0
// CHECK-NEXT: jump [[LOOP:bb[0-9]+]]
// CHECK: [[LOOP]]:
// CHECK-NEXT: dup 2
// CHECK-NEXT: jumpi {{bb[0-9]+}}, [[EXIT:bb[0-9]+]]
// CHECK: [[EXIT]]:
// CHECK-NEXT: swap 1
// CHECK-NEXT: pop
// CHECK-NEXT: swap 1
// CHECK-NEXT: jump{{$}}

contract TerminalTailEntryReturningModule {
    function checked(uint256 x) external pure returns (uint256) {
        require(x > 5, "literal msg");
        return x;
    }

    function other(uint256 x) external pure returns (uint256) {
        require(x > 5, "ERC20: insufficient allowance");
        return x;
    }

    function sum(uint256 n) external pure returns (uint256) {
        return folded(n & 7);
    }

    function folded(uint256 n) internal pure returns (uint256 total) {
        unchecked {
            while (n != 0) {
                total += n;
                --n;
            }
        }
    }
}
