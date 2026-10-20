//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[ir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: scaled 0, 7 => 0
//@ run-call: scaled 10, 7 => 315
//@ run-call: ascii 0x => true
//@ run-call: ascii 0x41427f => true
//@ run-call: ascii 0x414280 => false

contract Loops {
    // `n` and `k` are read from calldata once, before the loop.
    // CHECK-LABEL: @module Loops_runtime
    // CHECK: push 4{{$}}
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NOT: calldataload
    // CHECK: return
    function scaled(uint256 n, uint256 k) external pure returns (uint256 acc) {
        unchecked {
            for (uint256 i; i < n; ++i) acc += i * k;
        }
    }

    // The exit results are pushed after the branches that leave the loop, so the
    // loop carries no extra word and pops nothing. The latch runs the loop test
    // and its return.
    // CHECK: calldatacopy
    // CHECK: {{bb[0-9]+}} [loop]:
    // CHECK-NEXT: push 1{{$}}
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: sub
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 1{{$}}
    // CHECK-NEXT: push 0
    function ascii(bytes memory s) external pure returns (bool) {
        for (uint256 i; i < s.length; ++i) {
            if (uint8(s[i]) > 127) return false;
        }
        return true;
    }
}
