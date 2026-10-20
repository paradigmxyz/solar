//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: lastIndices [5, 6, 7], [9, 9, 9, 9] => false, [9, 0, 1, 2]
//@ run-call: lastIndices [5, 0, 7], [9, 9, 9, 9] => true, [9, 9, 9, 2]
//@ run-call: lastIndices [5], [9, 9, 9, 9] => false, [9, 9, 9, 9]
//@ run-call-fail: lastIndices [1, 2], [0] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// The early `return false` and the end of the loop share one block, which the
// loop leaves into besides its match. The loop's edge into that block gets a
// block of its own, and the loop is versioned like one with its own exits. The
// clone reads `a`'s length from before the loop, where `n` was read, so its
// reads of `a` keep no check.
// CHECK-LABEL: fn @_lastIndices(
// CHECK: shr {{v[0-9]+}}, 64
// CHECK: jumpi {{v[0-9]+}}, [[FAST:bb[0-9]+]], [[SLOW:bb[0-9]+]]
// CHECK: [[FAST]]:
// CHECK-NEXT: jump [[CLONE:bb[0-9]+]]
// CHECK: [[CLONE]]:
// CHECK-NOT: mload arg
// CHECK: [[I:v[0-9]+]] = sub {{v[0-9]+}}, 1
// CHECK-NEXT: shl [[I]], 5
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
