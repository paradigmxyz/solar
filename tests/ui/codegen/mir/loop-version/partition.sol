//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: partition [1, 5, 3, 9, 2, 8], 0, 4 => 3, 2, 9
//@ run-call: partition [5, 7, 9], 0, 6 => 1, 0, 12
//@ run-call: partition [9, 1, 7], 0, 5 => 2, 1, 14
//@ run-call-fail: partition [5, 1, 2], 0, 9 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: partition [9, 8], 0, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: partition [3, 3, 3, 3], 0, 3 => 2, 1, 0
//@ run-call: partition [0, 9, 1, 8, 2, 7, 3, 6, 4, 10], 0, 5 => 5, 4, 10
//@ run-call: sort [] => []
//@ run-call: sort [7] => [7]
//@ run-call: sort [2, 1] => [1, 2]
//@ run-call: sort [3, 1, 2] => [1, 2, 3]
//@ run-call: sort [4, 3, 2, 1] => [1, 2, 3, 4]
//@ run-call: sort [10, 9, 8, 7, 6, 5, 4, 3, 2, 1] => [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
//@ run-call: sort [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
//@ run-call: sort [5, 5, 5, 5, 5, 5, 5, 5, 5] => [5, 5, 5, 5, 5, 5, 5, 5, 5]
//@ run-call: sort [2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1] => [1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2]
//@ run-call: sort [30, 75, 69, 16, 47, 77, 60, 80, 74, 8, 77, 1, 60, 33, 70, 29, 24] => [1, 8, 16, 24, 29, 30, 33, 47, 60, 60, 69, 70, 74, 75, 77, 77, 80]
//@ run-call: sort [2, 1, 2, 2, 1, 1, 2, 0, 0, 2, 0, 2, 1, 2, 0, 2, 0] => [0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2]
//@ run-call: sort [20, 97, 75, 5, 38, 99, 3, 34, 60, 76, 92, 49, 91, 54, 50, 93, 73, 56, 17, 46, 12, 4, 17, 63, 27, 33, 86, 55, 99, 80, 38, 53, 64] => [3, 4, 5, 12, 17, 17, 20, 27, 33, 34, 38, 38, 46, 49, 50, 53, 54, 55, 56, 60, 63, 64, 73, 75, 76, 80, 86, 91, 92, 93, 97, 99, 99]
//@ run-call: sort [1, 2, 1, 2, 2, 1, 2, 0, 1, 2, 0, 1, 2, 2, 2, 0, 2, 1, 2, 2, 2, 0, 2, 2, 0, 2, 2, 1, 1, 0, 0, 1, 2] => [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2]
pragma solidity ^0.8.0;

// A Hoare partition's loop gets a copy whose scans run without their bounds checks, behind a test
// that the scans stop at the elements at both ends of the range; the swaps keep the elements they
// stop at in place for the next scans. A partition whose ends do not stop its scans runs the
// original loop, with its checks.
contract Partition {
    function sort(uint256[] memory a) external pure returns (uint256[] memory) {
        if (a.length > 1) _sort(a, 0, a.length);
        return a;
    }

    // Splits `a[lo:]` around a pivot the caller picks, reading both ends first but placing no
    // sentinels: when an end element does not stop its scan, the test fails and the original
    // loop runs, whose checks see a scan leave the array.
    function partition(uint256[] memory a, uint256 lo, uint256 y)
        external
        pure
        returns (uint256 i, uint256 j, uint256 ends)
    {
        i = lo;
        j = a.length - 1;
        ends = a[i] ^ a[j];
        while (true) {
            do {
                ++i;
            } while (a[i] < y);
            do {
                --j;
            } while (y < a[j]);
            if (i >= j) break;
            (a[i], a[j]) = (a[j], a[i]);
        }
    }

    // The partition's copy runs behind one test per partition that the elements the median of
    // three placed at both ends stop the scans. Its scans step pointers with no check of the
    // index against the length, or of `--j` against zero, its loop compares and swaps through
    // the pointers, and only its exit rebuilds `j` from how far the pointer moved.
    // CHECK-LABEL: fn @_sort(
    // CHECK: [[EITHER:v[0-9]+]] = or {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: jumpi [[EITHER]], {{bb[0-9]+}}, [[FAST:bb[0-9]+]]
    // CHECK: [[FAST]]:
    // CHECK: [[P:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK-NEXT: [[X:v[0-9]+]] = mload [[P]]
    // CHECK-NEXT: [[MORE:v[0-9]+]] = lt [[X]], {{v[0-9]+}}
    // CHECK-NEXT: jumpi [[MORE]], {{bb[0-9]+}}, {{bb[0-9]+}}
    // CHECK: [[Q:v[0-9]+]] = sub {{v[0-9]+}}, 32
    // CHECK-NEXT: [[Y:v[0-9]+]] = mload [[Q]]
    // CHECK: [[ORDERED:v[0-9]+]] = lt [[P]], [[Q]]
    // CHECK-NEXT: jumpi [[ORDERED]], [[SWAP:bb[0-9]+]], [[EXIT:bb[0-9]+]]
    // CHECK: [[EXIT]]:
    // CHECK-NEXT: [[MOVED:v[0-9]+]] = sub {{v[0-9]+}}, [[Q]]
    // CHECK: [[SWAP]]:
    // CHECK-NEXT: mload [[Q]]
    // CHECK-NEXT: mload [[P]]
    function _sort(uint256[] memory a, uint256 low, uint256 high) private pure {
        while (high - low > 3) {
            uint256 last = high - 1;
            uint256 mid = low + ((high - low) >> 1);
            uint256 x = a[low];
            uint256 y = a[mid];
            uint256 z = a[last];
            if (y < x) (x, y) = (y, x);
            if (z < y) {
                (y, z) = (z, y);
                if (y < x) (x, y) = (y, x);
            }
            (a[low], a[mid], a[last]) = (x, y, z);
            uint256 i = low;
            uint256 j = last;
            while (true) {
                do {
                    ++i;
                } while (a[i] < y);
                do {
                    --j;
                } while (y < a[j]);
                if (i >= j) break;
                (a[i], a[j]) = (a[j], a[i]);
            }
            if (j + 1 - low < high - j - 1) {
                _sort(a, low, j + 1);
                low = j + 1;
            } else {
                _sort(a, j + 1, high);
                high = j + 1;
            }
        }
        for (uint256 k = low + 1; k < high; ++k) {
            uint256 v = a[k];
            uint256 m = k;
            while (m > low && a[m - 1] > v) {
                a[m] = a[m - 1];
                --m;
            }
            a[m] = v;
        }
    }
}
