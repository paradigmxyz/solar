//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: find [], 7 => 0
//@ run-call: find [7], 7 => 0
//@ run-call: find [1], 7 => 1
//@ run-call: find [1, 7], 7 => 1
//@ run-call: find [7, 7], 7 => 0
//@ run-call: find [1, 2, 7], 7 => 2
//@ run-call: find [1, 2, 3], 7 => 3
//@ run-call: find [1, 2, 3, 7], 7 => 3
//@ run-call: find [1, 2, 3, 4, 7], 7 => 4
//@ run-call: find [1, 2, 3, 4, 5], 7 => 5
//@ run-call: findReturn [], 7 => 0
//@ run-call: findReturn [7], 7 => 0
//@ run-call: findReturn [1], 7 => 1
//@ run-call: findReturn [1, 7], 7 => 1
//@ run-call: findReturn [1, 2, 7], 7 => 2
//@ run-call: findReturn [1, 2, 3, 7], 7 => 3
//@ run-call: findReturn [1, 2, 3, 4, 5], 7 => 5
//@ run-call: count [1, 2, 0, 4], 3 => 2
//@ run-call: count [5, 6], 6 => 2
pragma solidity ^0.8.0;

// A search leaves its loop at a match as well as at the end, and still unrolls: every copy
// tests for a match, and the block after a match takes the pointer each copy reached through
// a phi, so the index it rebuilds is that copy's.
contract Search {
    // CHECK: [[ENTERED:v[0-9]+]] = phi [{{bb[0-9]+}}: [[START:v[0-9]+]]], [{{bb[0-9]+}}: {{v[0-9]+}}]
    // CHECK: [[POINTER:v[0-9]+]] = phi [{{bb[0-9]+}}: [[ENTERED]]], [{{bb[0-9]+}}: {{v[0-9]+}}]{{$}}
    // CHECK-NEXT: {{v[0-9]+}} = ne [[POINTER]],
    // CHECK: [[MATCHED:v[0-9]+]] = phi [{{bb[0-9]+}}: [[POINTER]]], [{{bb[0-9]+}}: [[START]]], [{{bb[0-9]+}}: {{v[0-9]+}}]
    // CHECK-NEXT: [[DELTA:v[0-9]+]] = sub [[MATCHED]], [[START]]
    // CHECK-NEXT: {{v[0-9]+}} = shr 5, [[DELTA]]
    function find(uint256[] calldata values, uint256 x) external pure returns (uint256 found) {
        found = values.length;
        for (uint256 i = 0; i < values.length; ++i) {
            if (values[i] == x) {
                found = i;
                break;
            }
        }
    }

    // Returns at its match, so the match exit takes the pointer through a phi in a block
    // that returns rather than continues.
    function findReturn(uint256[] calldata values, uint256 x) external pure returns (uint256) {
        for (uint256 i = 0; i < values.length; ++i) {
            if (values[i] == x) return i;
        }
        return values.length;
    }

    // Stops at the first zero, as a match, or counts up to `limit` elements.
    function count(uint256[] calldata values, uint256 limit) external pure returns (uint256 n) {
        for (uint256 i = 0; i < values.length && i < limit; ++i) {
            if (values[i] == 0) break;
            ++n;
        }
    }
}
