//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

contract Base {
    uint256 private immutable value = 1;

    function baseValue() external pure returns (uint256) {
        return value;
    }
}

// CHECK-LABEL: @module Derived
// CHECK-NEXT: immutables:
// CHECK-NEXT: value0: u256
// CHECK-NEXT: value1: u256
//
// CHECK-LABEL: fn @constructor{{[( ]}}
// CHECK: storeimmutable value0, 1
// CHECK-NEXT: storeimmutable value1, 2
//
// CHECK-LABEL: fn @derivedValue{{[( ]}}
// CHECK: loadimmutable value1
//
// CHECK-LABEL: fn @baseValue{{[( ]}}
// CHECK: loadimmutable value0
contract Derived is Base {
    uint256 private immutable value = 2;

    function derivedValue() external pure returns (uint256) {
        return value;
    }
}
