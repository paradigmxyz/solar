//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=lower-integers -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: wrap8 200, 100 => 44
//@[run] run-call: lt8 -1, 1 => true
//@[run] run-call: lt8 1, -1 => false
//@[run] run-call: widen -5 => -5
//@[run] run-call: narrow 0x12345 => 0x2345
//@[run] run-call: word 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 1
//@[run] run-call: flag true, false => false
// Narrow integers widen to words: arithmetic masks its result to the declared width, and signed
// comparisons and widening sign-extend their operands. Word-sized integers and booleans keep
// their operations.

contract NarrowIntegers {
    // CHECK-LABEL: fn @wrap8(
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK-NEXT: and [[SUM]], 255
    function wrap8(uint8 a, uint8 b) public pure returns (uint8) {
        unchecked {
            return a + b;
        }
    }

    // CHECK-LABEL: fn @lt8(
    // CHECK: [[A:v[0-9]+]] = signextend 0, arg0
    // CHECK-NEXT: [[B:v[0-9]+]] = signextend 0, arg1
    // CHECK-NEXT: slt [[A]], [[B]]
    function lt8(int8 a, int8 b) public pure returns (bool) {
        return a < b;
    }

    // CHECK-LABEL: fn @widen(
    // CHECK: signextend 0, arg0
    function widen(int8 a) public pure returns (int256) {
        return a;
    }

    // CHECK-LABEL: fn @narrow(
    // CHECK: and arg0, 0xffff
    function narrow(uint256 a) public pure returns (uint16) {
        return uint16(a);
    }

    // CHECK-LABEL: fn @word(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK-NEXT: ret [[SUM]]
    function word(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }

    // CHECK-LABEL: fn @flag(
    // CHECK-SAME: arg0: i1, arg1: i1) -> i1
    // CHECK-NOT: and
    // CHECK: phi [bb1: arg1], [bb2: false]
    function flag(bool a, bool b) public pure returns (bool) {
        return a && b;
    }
}
