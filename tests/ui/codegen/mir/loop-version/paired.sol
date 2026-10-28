//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: sortPairs [3, 1, 2], [30, 10, 20] => [1, 2, 3], [10, 20, 30]
//@ run-call: sortPairs [], [] => [], []
//@ run-call-fail: sortPairs [2, 1], [20] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: sameArray [3, 1, 2] => [1, 2, 3]
//@ run-call: overlapping [4, 5, 3, 4, 5, 0, 6, 1] => [3, 4, 4, 5, 0, 1, 5, 6]

// Moving a value with each key stores into both arrays, so the original loop
// reloads both lengths before every checked access. The copy reads them once in
// the test that guards it, and its inner loop keeps no bounds check.
// CHECK-LABEL: fn @_sortPairs(
// CHECK: shr {{.*}}, 64
// CHECK: jumpi [[OK:v[0-9]+]], [[FAST:bb[0-9]+]], [[SLOW:bb[0-9]+]]
// CHECK: [[FAST]]:
// CHECK-NOT: mload arg
// CHECK: ret

contract Test {
    function sortPairs(uint256[] memory keys, uint256[] memory values)
        external
        pure
        returns (uint256[] memory, uint256[] memory)
    {
        _sortPairs(keys, values);
        return (keys, values);
    }

    // Both arrays are one object: the test passes, and every store writes both.
    function sameArray(uint256[] memory a) external pure returns (uint256[] memory) {
        _sortPairs(a, a);
        return a;
    }

    // The keys are a view into the values whose length word is `a[2]`, which the
    // value stores change: the test fails and the original loop runs.
    function overlapping(uint256[] memory a) external pure returns (uint256[] memory) {
        uint256[] memory keys;
        assembly ("memory-safe") {
            keys := add(a, 0x60)
        }
        _sortPairs(keys, a);
        return a;
    }

    function _sortPairs(uint256[] memory keys, uint256[] memory values) private pure {
        for (uint256 i = 1; i < keys.length; ++i) {
            uint256 k = keys[i];
            uint256 v = values[i];
            uint256 j = i;
            while (j > 0 && keys[j - 1] > k) {
                keys[j] = keys[j - 1];
                values[j] = values[j - 1];
                --j;
            }
            keys[j] = k;
            values[j] = v;
        }
    }
}
