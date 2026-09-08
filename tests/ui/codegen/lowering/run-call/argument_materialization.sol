//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: ArgumentMaterialization::addCarry 0 => 17
//@ run-call-fail: ArgumentMaterialization::addCarry 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x
//@ run-call: ArgumentMaterialization::mulCarry 2 => 34
//@ run-call-fail: ArgumentMaterialization::mulCarry 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x
//@ run-call: ArgumentMaterialization::andCarry 255 => 255
//@ run-call-fail: ArgumentMaterialization::andCarry 256 => 0x
//@ run-call: ArgumentMaterialization::orCarry 256 => 256
//@ run-call-fail: ArgumentMaterialization::orCarry 0 => 0x
//@ run-call: ArgumentMaterialization::xorCarry 0 => 256
//@ run-call-fail: ArgumentMaterialization::xorCarry 256 => 0x
//@ run-call: ArgumentMaterialization::eqCarry 0 => 0
//@ run-call-fail: ArgumentMaterialization::eqCarry 17 => 0x
//@ run-call: ArgumentMaterializationRefusals::wrongArgument 0, 17 => 17
//@ run-call-fail: ArgumentMaterializationRefusals::wrongArgument 0, 18 => 0x
//@ run-call: ArgumentMaterializationRefusals::subtract 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffef
//@ run-call-fail: ArgumentMaterializationRefusals::subtract 17 => 0x

// Each producer keeps its result while the ordered comparison reuses its input.
// These checks require native carry activation for every producer function.
// CHECK-LABEL: @module ArgumentMaterialization_runtime
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 17
// CHECK-NEXT: mul
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 256
// CHECK-NEXT: or
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 255
// CHECK-NEXT: and
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 256
// CHECK-NEXT: xor
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 17
// CHECK-NEXT: eq
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 1
// CHECK-NEXT: push 17
// CHECK-NEXT: add
// CHECK-NEXT: swap 1
// CHECK-NEXT: dup 2
// CHECK-NEXT: lt
// CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
contract ArgumentMaterialization {
    function addCarry(uint256 x) external pure returns (uint256 p) {
        unchecked { p = x + 17; }
        if (p < x) revert();
    }

    function mulCarry(uint256 x) external pure returns (uint256 p) {
        unchecked { p = x * 17; }
        if (p < x) revert();
    }

    function andCarry(uint256 x) external pure returns (uint256 p) {
        p = x & 255;
        if (p < x) revert();
    }

    function orCarry(uint256 x) external pure returns (uint256 p) {
        p = x | 256;
        if (p > x) revert();
    }

    function xorCarry(uint256 x) external pure returns (uint256 p) {
        p = x ^ 256;
        if (p < x) revert();
    }

    function eqCarry(uint256 x) external pure returns (uint256 p) {
        assembly { p := eq(x, 17) }
        if (p < x) revert();
    }
}

// A different Arg identity and noncommutative producer must retain their order.
contract ArgumentMaterializationRefusals {
    function wrongArgument(uint256 x, uint256 z) external pure returns (uint256 p) {
        unchecked { p = x + 17; }
        if (p < z) revert();
    }

    function subtract(uint256 x) external pure returns (uint256 p) {
        unchecked { p = x - 17; }
        if (p < x) revert();
    }
}
