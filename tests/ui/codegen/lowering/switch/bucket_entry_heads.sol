//@ revisions: lifetime once
//@ compile-flags: -O gas -Zswitch-lowering=buckets -Zdump=evm-ir-runtime,disasm-runtime
//@[lifetime] compile-flags: --optimize-runs 1000000
//@[lifetime] filecheck: --check-prefix=HEADS
//@[once] compile-flags: --optimize-runs 1
//@[once] filecheck: --check-prefix=PLAIN
//@ run-call: f00 => 0
//@ run-call: f01 => 1
//@ run-call: f02 => 2
//@ run-call: f03 => 3
//@ run-call: f04 => 4
//@ run-call: f05 => 5
//@ run-call: f06 => 6
//@ run-call: f07 => 7
//@ run-call: f08 => 8
//@ run-call: f09 => 9
//@ run-call: f10 => 10
//@ run-call: f11 => 11
//@ run-call: f12 => 12
//@ run-call: f13 => 13
//@ run-call: f14 => 14
//@ run-call: f15 => 15
//@ run-call: f16 => 16
//@ run-call: f17 => 17
//@ run-call: f18 => 18
//@ run-call: f19 => 19
//@ run-call: f20 => 20
//@ run-call: f21 => 21
//@ run-call: f22 => 22
//@ run-call: f23 => 23
//@ run-call: f24 => 24
//@ run-call: f25 => 25
//@ run-call: f26 => 26
//@ run-call: f27 => 27
//@ run-call: f28 => 28
//@ run-call: f29 => 29
//@ run-call: f30 => 30
//@ run-call: f31 => 31
//@ run-call: f32 => 32
//@ run-call: f33 => 33
//@ run-call: f34 => 34
//@ run-call: f35 => 35
//@ run-call: f36 => 36
//@ run-call: f37 => 37
//@ run-call: f38 => 38
//@ run-call: f39 => 39
//@ run-call-fail: 0x12345678

// Over many calls, a bucket's first comparison moves into its table entry,
// and every entry pads to one stride. Once, the entries only jump.
// HEADS-LABEL: @module BucketEntryHeads_runtime
// HEADS: jumpi bb{{[0-9]+}}, bb{{[0-9]+}}
// HEADS-NEXT: {{^}}bb{{[0-9]+}}:
// HEADS-NEXT: dup 1
// HEADS-NEXT: push 0x{{[0-9a-f]+}}
// HEADS-NEXT: eq
// HEADS-NEXT: jumpi bb{{[0-9]+}}, bb{{[0-9]+}}
// HEADS: MOD
// HEADS-NEXT: PUSH1 0x04
// HEADS-NEXT: SHL
// HEADS: INVALID
// PLAIN-LABEL: @module BucketEntryHeads_runtime
// PLAIN-NOT: jumpi bb{{[0-9]+}}, bb{{[0-9]+}}
// PLAIN: MOD
// PLAIN-NEXT: PUSH1 0x05
// PLAIN-NEXT: MUL
// PLAIN-NOT: INVALID
contract BucketEntryHeads {
    function f00() external pure returns (uint256) {
        return 0;
    }

    function f01() external pure returns (uint256) {
        return 1;
    }

    function f02() external pure returns (uint256) {
        return 2;
    }

    function f03() external pure returns (uint256) {
        return 3;
    }

    function f04() external pure returns (uint256) {
        return 4;
    }

    function f05() external pure returns (uint256) {
        return 5;
    }

    function f06() external pure returns (uint256) {
        return 6;
    }

    function f07() external pure returns (uint256) {
        return 7;
    }

    function f08() external pure returns (uint256) {
        return 8;
    }

    function f09() external pure returns (uint256) {
        return 9;
    }

    function f10() external pure returns (uint256) {
        return 10;
    }

    function f11() external pure returns (uint256) {
        return 11;
    }

    function f12() external pure returns (uint256) {
        return 12;
    }

    function f13() external pure returns (uint256) {
        return 13;
    }

    function f14() external pure returns (uint256) {
        return 14;
    }

    function f15() external pure returns (uint256) {
        return 15;
    }

    function f16() external pure returns (uint256) {
        return 16;
    }

    function f17() external pure returns (uint256) {
        return 17;
    }

    function f18() external pure returns (uint256) {
        return 18;
    }

    function f19() external pure returns (uint256) {
        return 19;
    }

    function f20() external pure returns (uint256) {
        return 20;
    }

    function f21() external pure returns (uint256) {
        return 21;
    }

    function f22() external pure returns (uint256) {
        return 22;
    }

    function f23() external pure returns (uint256) {
        return 23;
    }

    function f24() external pure returns (uint256) {
        return 24;
    }

    function f25() external pure returns (uint256) {
        return 25;
    }

    function f26() external pure returns (uint256) {
        return 26;
    }

    function f27() external pure returns (uint256) {
        return 27;
    }

    function f28() external pure returns (uint256) {
        return 28;
    }

    function f29() external pure returns (uint256) {
        return 29;
    }

    function f30() external pure returns (uint256) {
        return 30;
    }

    function f31() external pure returns (uint256) {
        return 31;
    }

    function f32() external pure returns (uint256) {
        return 32;
    }

    function f33() external pure returns (uint256) {
        return 33;
    }

    function f34() external pure returns (uint256) {
        return 34;
    }

    function f35() external pure returns (uint256) {
        return 35;
    }

    function f36() external pure returns (uint256) {
        return 36;
    }

    function f37() external pure returns (uint256) {
        return 37;
    }

    function f38() external pure returns (uint256) {
        return 38;
    }

    function f39() external pure returns (uint256) {
        return 39;
    }
}
