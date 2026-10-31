//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: lastIndices [5, 6, 7], [9, 9, 9, 9] => false, [9, 0, 1, 2]
//@ run-call: lastIndices [5, 0, 7], [9, 9, 9, 9] => true, [9, 9, 9, 2]
//@ run-call: lastIndices [5], [9, 9, 9, 9] => false, [9, 9, 9, 9]
//@ run-call-fail: lastIndices [1, 2], [0] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// The early `return false` and the end of the loop share one block, which the
// loop leaves into besides its match. The loop's edge into that block gets a
// block of its own, and the loop is versioned like one with its own exits. The
// loop reads `last`'s length only after the first element's check and match,
// so the test runs on the back edge, and the clone reloads no length.
// CHECK-LABEL: fn @_lastIndices(
// CHECK: jump [[HEADER:bb[0-9]+]]
// CHECK-NEXT: [[HEADER]]:
// CHECK: lt {{v[0-9]+}}, 0x10000000000000000
// CHECK: jumpi {{v[0-9]+}}, [[FAST:bb[0-9]+]], [[HEADER]]
// CHECK: [[FAST]]:
// CHECK: jump [[CLONE:bb[0-9]+]]
// CHECK: [[CLONE]]:
// CHECK-NOT: mload arg
// CHECK: ret

contract Test {
    function lastIndices(uint256[] memory a, uint256[] memory last)
        external
        pure
        returns (bool, uint256[] memory)
    {
        bool zero = _lastIndices(a, last);
        return (zero, last);
    }

    // Counting down, the loop tests `i != 0`, so each read of `a` keeps its
    // bounds check against a length the stores into `last` might reach.
    function _lastIndices(uint256[] memory a, uint256[] memory last) private pure returns (bool) {
        uint256 n = a.length;
        if (n < 2) return false;
        for (uint256 i = n; i != 0;) {
            --i;
            uint256 v = a[i];
            if (v == 0) return true;
            last[v & 3] = i;
        }
        return false;
    }
}
