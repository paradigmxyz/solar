//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: siftUp 7, 7 => [3, 7, 2, 10, 6, 105, 9], [2, 1, 5, 3, 4, 0, 6]
//@ run-call: siftUp 7, 5 => [3, 7, 102, 10, 6, 2, 9], [2, 1, 0, 3, 4, 5, 6]
//@ run-call: siftUp 1, 1 => [100], [0]
//@ run-call: siftUp 12, 12 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111], [2, 1, 5, 3, 4, 11, 6, 7, 8, 9, 10, 0]
//@ run-call: siftUp 12, 9 => [3, 7, 2, 10, 6, 105, 9, 5, 1, 8, 4, 0], [2, 1, 5, 3, 4, 0, 6, 7, 8, 9, 10, 11]
//@ run-call: siftUp 7, 9 => [3, 7, 2, 10, 6, 105, 9], [2, 1, 5, 3, 4, 0, 6]
//@ run-call: siftUp 16, 17 => [3, 7, 2, 10, 6, 0, 9, 5, 1, 8, 4, 111, 7, 3, 10, 6], [2, 1, 5, 3, 4, 11, 6, 7, 8, 9, 10, 0, 12, 13, 14, 15]
//@ run-call-fail: siftUp 3, 6 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
pragma solidity ^0.8.0;

// A counted loop whose bound is not the length of the arrays it checks its accesses against
// gets a copy without those checks, behind a test that the bound does not exceed the lengths.
// A bound past a length runs the original loop, with its checks.
contract Bounds {
    // A heap's sift loop runs up to the queue's size `e`, while every access checks its index
    // against the length of an array that holds the queue. The copy runs behind
    // `e <= values.length && e <= keys.length`, which both lengths read before the loop.
    // CHECK-LABEL: fn @siftUp(
    // CHECK: {{v[0-9]+}} = gt [[VALUES:v[0-9]+]], 0{{$}}
    // CHECK-NEXT: jumpi {{v[0-9]+}}, {{bb[0-9]+}}, [[PANIC:bb[0-9]+]]
    // CHECK: {{v[0-9]+}} = gt [[KEYS:v[0-9]+]], 0{{$}}
    // CHECK: [[SHORT_VALUES:v[0-9]+]] = lt [[VALUES]], arg1
    // CHECK-NEXT: [[SHORT_KEYS:v[0-9]+]] = lt [[KEYS]], arg1
    // CHECK-NEXT: [[SHORT:v[0-9]+]] = or [[SHORT_VALUES]], [[SHORT_KEYS]]
    // CHECK-NEXT: jumpi [[SHORT]], [[SLOW:bb[0-9]+]], [[FAST:bb[0-9]+]]
    // CHECK-NEXT: [[FAST]]:
    // CHECK-NOT: [[PANIC]]
    // CHECK: [[SLOW]]:
    // CHECK: jumpi {{v[0-9]+}}, {{bb[0-9]+}}, [[PANIC]]
    function siftUp(uint256 m, uint256 e)
        external
        pure
        returns (uint256[] memory values, uint256[] memory keys)
    {
        values = new uint256[](m);
        keys = new uint256[](m);
        for (uint256 i; i < m; ++i) {
            values[i] = (i * 7) % 11;
            keys[i] = i;
        }
        values[0] = 100;
        uint256 k0 = keys[0];
        uint256 p;
        for (uint256 c = 1; c < e; c = (p << 1) | 1) {
            uint256 h = c >> 1;
            uint256 cv = values[c];
            if (c + 1 < e) {
                uint256 rv = values[c + 1];
                if (cv > rv) {
                    cv = rv;
                    ++c;
                }
            }
            values[h] = cv;
            keys[h] = keys[c];
            p = c;
        }
        values[p] = 100 + p;
        keys[p] = k0;
    }
}
