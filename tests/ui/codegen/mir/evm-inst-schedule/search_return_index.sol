//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: last [], 7 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: last [7], 7 => 0
//@ run-call: last [7], 8 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: last [7, 1, 2, 3, 4, 5, 6, 8], 7 => 0
//@ run-call: last [1, 7, 2, 3, 4, 5, 6, 8], 7 => 1
//@ run-call: last [1, 2, 7, 3, 4, 5, 6, 8], 7 => 2
//@ run-call: last [1, 2, 3, 7, 4, 5, 6, 8], 7 => 3
//@ run-call: last [1, 2, 3, 4, 7, 5, 6, 8], 7 => 4
//@ run-call: last [1, 2, 3, 4, 5, 7, 6, 8], 7 => 5
//@ run-call: last [1, 2, 3, 4, 5, 6, 7, 8], 7 => 6
//@ run-call: last [1, 2, 3, 4, 5, 6, 8, 7], 7 => 7
//@ run-call: last [7, 2, 3, 4, 5, 6, 8, 7], 7 => 7
//@ run-call: last [1, 2, 3, 4, 5, 6, 8, 9], 7 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
pragma solidity ^0.8.4;

// Each test of the unrolled search computes `i - k` only for the return that reads it, after
// the element at `i - k` matched: the return blocks, not the testing ones, subtract.
contract SearchReturnIndex {
    // CHECK-LABEL: fn @last()
    // CHECK: = eq {{v[0-9]+}}, arg1
    // CHECK: [[WORD:v[0-9]+]] = mload {{v[0-9]+}}
    // CHECK-NEXT: [[EQUAL:v[0-9]+]] = eq [[WORD]], arg1
    // CHECK-NEXT: jumpi [[EQUAL]], [[FOUND:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: [[FOUND]]:
    // CHECK-NEXT: [[INDEX:v[0-9]+]] = sub {{v[0-9]+}}, 1
    // CHECK-NEXT: mstore 0, [[INDEX]]
    function last(uint256[] memory a, uint256 x) external pure returns (uint256) {
        uint256 i = a.length;
        if (i == 0) return type(uint256).max;
        --i;
        while (i >= 4) {
            if (a[i] == x) return i;
            if (a[i - 1] == x) return i - 1;
            if (a[i - 2] == x) return i - 2;
            if (a[i - 3] == x) return i - 3;
            i -= 4;
        }
        while (a[i] != x) {
            if (i == 0) return type(uint256).max;
            --i;
        }
        return i;
    }
}
