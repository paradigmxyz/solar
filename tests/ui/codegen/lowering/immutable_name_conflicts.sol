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
// CHECK-NEXT: value0_: u256
// CHECK-NEXT: value1: u256
// CHECK-NEXT: value0: u256
//
// CHECK-LABEL: fn @constructor{{[( ]}}
// CHECK: storeimmutable value0_, 1
// CHECK-NEXT: storeimmutable value1, 2
// CHECK-NEXT: storeimmutable value0, 3
//
// CHECK-LABEL: fn @derivedValue{{[( ]}}
// CHECK: loadimmutable value1
//
// CHECK-LABEL: fn @numberedValue{{[( ]}}
// CHECK: loadimmutable value0
//
// CHECK-LABEL: fn @baseValue{{[( ]}}
// CHECK: loadimmutable value0_
contract Derived is Base {
    uint256 private immutable value = 2;
    uint256 private immutable value0 = 3;

    function derivedValue() external pure returns (uint256) {
        return value;
    }

    function numberedValue() external pure returns (uint256) {
        return value0;
    }
}
