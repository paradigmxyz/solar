//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: swap 0, 0 => 0, 0
//@ run-call: swap 7, 9 => 9, 7
//@ run-call: swap 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 1, 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: multi => 7, 9
//@ run-call: viaNamed 0x0000000000000000000000000000000000000004, 0x0102 => true
//@ run-call: viaNamed 0x0000000000000000000000000000000000000009, 0x => false
//@ run-call-fail: 0x => 0x
//@ run-call-fail: 0xdeadbeef => 0x
//@ run-call-fail: 0xd96073cf => 0x
//@ run-call-fail: 0x5030da75 => 0x
//@ run-call-fail: multi; value=1 => 0x
//@ run-call-fail: 0x5030da75000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call-fail: 0x5030da75000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call-fail: 0x5030da75000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000021 => 0x

// Assignment to existing tuple lvalues evaluates the complete RHS before writes.
// The ordinary multi-return leaf exposes (7, 9); low-level assignment returns
// the CALL success flag, including false when the callee fails.
//
// Compact direct returns replace the old shared second-word-store/return block.
// This is a measured Gas-over-sharing policy: swap is 7 gas below the sealed
// reference and multi is 41 below it. The literal pointer store at scratch32
// remains. This is not label normalization or elimination of all frame traffic.
// viaNamed still costs 10 more gas than sealed Gas and 7 more than sealed Size.
//
// The strict IR revision also retains all decoder arithmetic and stack operations.
// Selector edges, tuple word order, CALL data flow, and direct returns are checked
// below in emitted block order. The standard matrix exercises the runtime oracles.

// CHECK-LABEL: @module C_runtime
// CHECK: [[ENTRY:bb[0-9]+]]:
// CHECK-NEXT: push 192
// CHECK-NEXT: push 64
// CHECK-NEXT: mstore
// CHECK-NEXT: callvalue
// CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[DISPATCH:bb[0-9]+]]
// CHECK: [[DISPATCH]]:
// CHECK-NEXT: push 0
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 224
// CHECK-NEXT: shr
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0x1b8f5d50
// CHECK-NEXT: eq
// CHECK-NEXT: jumpi [[MULTI:bb[0-9]+]], [[NEXT_SELECTOR:bb[0-9]+]]
// CHECK: [[NEXT_SELECTOR]]:
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 0x5030da75
// CHECK-NEXT: eq
// CHECK-NEXT: jumpi [[NAMED:bb[0-9]+]], [[SWAP_SELECTOR:bb[0-9]+]]
// CHECK: [[SWAP_SELECTOR]]:
// CHECK-NEXT: push 0xd96073cf
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi [[REJECT]], [[SWAP_GUARD:bb[0-9]+]]
// CHECK: [[SWAP_GUARD]]:
// CHECK-NEXT: calldatasize
// CHECK-NEXT: push 68
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi [[REJECT]], [[SWAP:bb[0-9]+]]
// CHECK: [[SWAP]]:
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: push 64
// CHECK-NEXT: push 0
// CHECK-NEXT: return
// CHECK: [[REJECT]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
// CHECK: [[NAMED]]:
// CHECK-NEXT: pop
// CHECK-NEXT: calldatasize
// CHECK-NEXT: push 68
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi [[REJECT]], [[ADDRESS_GUARD:bb[0-9]+]]
// CHECK: [[ADDRESS_GUARD]]:
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 160
// CHECK-NEXT: shr
// CHECK-NEXT: jumpi [[REJECT]], [[BYTES_GUARD:bb[0-9]+]]
// CHECK: [[BYTES_GUARD]]:
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK: jumpi [[REJECT]], [[LENGTH_GUARD:bb[0-9]+]]
// CHECK: [[LENGTH_GUARD]]:
// CHECK-NEXT: gas
// CHECK-NEXT: push 0
// CHECK-NEXT: not
// CHECK-NEXT: push 192
// CHECK-NEXT: shr
// CHECK-NEXT: dup 3
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi [[REJECT]], [[COPY_GUARD:bb[0-9]+]]
// CHECK: [[COPY_GUARD]]:
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 1
// CHECK-NEXT: calldatasize
// CHECK-NEXT: dup 2
// CHECK-NEXT: swap 1
// CHECK-NEXT: sub
// CHECK-NEXT: dup 3
// CHECK-NEXT: sgt
// CHECK-NEXT: jumpi [[REJECT]], [[CALL_BODY:bb[0-9]+]]
// CHECK: [[CALL_BODY]]:
// CHECK-NEXT: push 63
// CHECK: push 32
// CHECK-NEXT: dup 2
// CHECK-NEXT: add
// CHECK-NEXT: exchange 1, 3
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 3
// CHECK-NEXT: calldatacopy
// CHECK-NEXT: swap 1
// CHECK-NEXT: mload
// CHECK-NEXT: push 0
// CHECK-NEXT: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 0
// CHECK-NEXT: push 0
// CHECK-NEXT: swap 5
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 6
// CHECK-NEXT: exchange 1, 2
// CHECK-NEXT: call
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: return
// CHECK: [[MULTI]]:
// CHECK-NEXT: push 128
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: push 7
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 9
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: push 64
// CHECK-NEXT: push 0
// CHECK-NEXT: return

contract C {
    function viaNamed(address t, bytes calldata d) external returns (bool ok) {
        (ok, ) = t.call(d);
    }
    function swap(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        (a, b) = (b, a);
        return (a, b);
    }
    function two() internal pure returns (uint256, uint256) {
        return (7, 9);
    }
    function multi() external pure returns (uint256 x, uint256 y) {
        x = 100;
        y = 200;
        (x, y) = two();
    }
}
