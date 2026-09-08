//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[none,gas,size,mir,ir] run-call: select 0x0000000000000000000000000000000000000001, 0; gas=1000000 => 1
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000001, 115792089237316195423570985008687907853269984665640564039457584007913129639934; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000001, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000002, 0; gas=1000000 => 2
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000002, 115792089237316195423570985008687907853269984665640564039457584007913129639933; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000002, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000003, 0; gas=1000000 => 3
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000003, 115792089237316195423570985008687907853269984665640564039457584007913129639932; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000003, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000004, 0; gas=1000000 => 4
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000004, 115792089237316195423570985008687907853269984665640564039457584007913129639931; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000004, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000005, 0; gas=1000000 => 5
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000005, 115792089237316195423570985008687907853269984665640564039457584007913129639930; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000005, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000000, 0; gas=1000000 => 0
//@[none,gas,size,mir] run-call: select 0x0000000000000000000000000000000000000000, 115792089237316195423570985008687907853269984665640564039457584007913129639935; gas=1000000 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@[none,gas,size,mir] run-call-fail: 0xffffffff; gas=1000000 => 0x
//@[none,gas,size,mir] run-call-fail: 0xc21f7bbb; gas=1000000 => 0x
//@[none,gas,size,mir] run-call-fail: select 0x0000000000000000000000000000000000000000, 0; gas=1000000, value=1 => 0x
//@[none,gas,size,mir] run-call-fail: 0xc21f7bbb00000000000001000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000; gas=1000000 => 0x

// One cleaned account stays on the stack across the branch chain. Each
// selected arm reads the independent value word and preserves checked addition.
// Branch polarity and eager value loading from the old scheduler are not required.
// Historical sealed byte-size debt is tracked separately; this test is not a
// claim that the old shared-tail size has been recovered.
contract Test {
    // CHECK-LABEL: @module Test_runtime
    // CHECK: callvalue
    // CHECK-NEXT: jumpi [[INVALID:bb[0-9]+]], [[SELECTOR:bb[0-9]+]]
    // CHECK: [[SELECTOR]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 224
    // CHECK-NEXT: shr
    // CHECK-NEXT: push 0xc21f7bbb
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[INVALID]], [[HEAD:bb[0-9]+]]
    // CHECK: [[HEAD]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[INVALID]], [[ADDRESS:bb[0-9]+]]
    // CHECK: [[ADDRESS]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 160
    // CHECK-NEXT: shr
    // CHECK-NEXT: jumpi [[INVALID]], [[CHAIN:bb[0-9]+]]
    // CHECK: [[CHAIN]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: not
    // CHECK-NEXT: push 96
    // CHECK-NEXT: shr
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: and
    // CHECK-NEXT: push 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ONE:bb[0-9]+]], [[NEXT2:bb[0-9]+]]
    // CHECK-NEXT: [[NEXT2]]:
    // CHECK-NEXT: push 2
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[TWO:bb[0-9]+]], [[NEXT3:bb[0-9]+]]
    // CHECK-NEXT: [[NEXT3]]:
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[THREE:bb[0-9]+]], [[NEXT4:bb[0-9]+]]
    // CHECK-NEXT: [[NEXT4]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[FOUR:bb[0-9]+]], [[NEXT5:bb[0-9]+]]
    // CHECK-NEXT: [[NEXT5]]:
    // CHECK-NEXT: push 5
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[FIVE:bb[0-9]+]], [[DEFAULT:bb[0-9]+]]
    // CHECK-NEXT: [[DEFAULT]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[OVERFLOW:bb[0-9]+]] [cold]:
    // CHECK-NEXT: push 0x4e487b71
    // CHECK-NEXT: push 224
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 17
    // CHECK-NEXT: push 4
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 36
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[FIVE]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push 5
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[OVERFLOW]], {{bb[0-9]+}}
    // CHECK: [[INVALID]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[TWO]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push 2
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[OVERFLOW]], {{bb[0-9]+}}
    // CHECK: [[THREE]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push 3
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[OVERFLOW]], {{bb[0-9]+}}
    // CHECK: [[FOUR]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[OVERFLOW]], {{bb[0-9]+}}
    // CHECK: [[ONE]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: iszero
    // CHECK-NEXT: jumpi [[OVERFLOW]], {{bb[0-9]+}}
    function select(address account, uint256 value) external pure returns (uint256) {
        if (account == address(1)) return value + 1;
        if (account == address(2)) return value + 2;
        if (account == address(3)) return value + 3;
        if (account == address(4)) return value + 4;
        if (account == address(5)) return value + 5;
        return value;
    }
}
