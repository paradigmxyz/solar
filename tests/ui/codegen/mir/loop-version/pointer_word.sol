//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: lastIndicesIn [5, 6, 7], [1], [9, 9, 9, 9] => false, [2, 9, 0, 1]
//@ run-call: lastIndicesIn [5, 0, 7], [1], [9, 9, 9, 9] => true, [2, 9, 9, 9]
//@ run-call-fail: lastIndicesIn [5], [], [9, 9, 9, 9] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// The loop reads the array out of the struct again after every store, as a
// pointer. The versioning test reads it once, still as a pointer, so the
// copy's element addresses built from it keep their type.
// CHECK-LABEL: fn @_lastIndicesIn(
// CHECK: lt {{v[0-9]+}}, 0x10000000000000000
// CHECK: jumpi {{v[0-9]+}}, [[FAST:bb[0-9]+]], [[SLOW:bb[0-9]+]]
// CHECK: [[FAST]]:
// CHECK: jump [[CLONE:bb[0-9]+]]
// CHECK: [[CLONE]]:
// CHECK-NOT: mload arg
// CHECK: ret

contract Test {
    struct Bag {
        uint256[] items;
    }

    function lastIndicesIn(uint256[] memory a, uint256[] memory items, uint256[] memory last)
        external
        pure
        returns (bool, uint256[] memory)
    {
        bool zero = _lastIndicesIn(Bag(items), a, last);
        return (zero, last);
    }

    function _lastIndicesIn(Bag memory bag, uint256[] memory a, uint256[] memory last)
        private
        pure
        returns (bool zero)
    {
        for (uint256 i = a.length; i != 0;) {
            --i;
            uint256 v = a[i];
            if (v == 0) {
                zero = true;
                break;
            }
            last[(v + bag.items[0]) & 3] = i;
        }
    }
}
