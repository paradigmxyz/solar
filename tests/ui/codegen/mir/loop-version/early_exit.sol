//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: splitUntilZero [5, 0, 7], [0, 0, 0], [0, 0, 0] => 1, [5, 0, 0], [0, 0, 0]
//@ run-call: splitUntilZero [340282366920938463463374607431768211459, 2], [0, 0], [0, 0] => 2, [3, 2], [1, 0]
//@ run-call-fail: splitUntilZero [1, 2], [0], [0, 0] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: overlappingSplit [7, 0], [3, 3, 3, 3, 3, 3] => 1, [7, 3, 0, 3, 3, 3], [0, 3, 3]
//@ run-call-fail: overlappingSplit [7, 1, 0], [3, 3, 3, 3, 3, 3] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A loop with an early return leaves through two exits besides its failed
// checks. Each exit merges the two copies' values in a phi of its own.
// CHECK-LABEL: fn @_splitUntilZero(
// CHECK: shr {{v[0-9]+}}, 64
// CHECK: jumpi {{v[0-9]+}}, [[FAST:bb[0-9]+]], [[SLOW:bb[0-9]+]]
// CHECK: [[FAST]]:
// CHECK-NEXT: jump [[CLONE:bb[0-9]+]]
// CHECK: [[CLONE]]:
// CHECK-NEXT: phi
// CHECK-NOT: mload arg
// CHECK: phi [{{bb[0-9]+}}: {{v[0-9]+}}], [{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK-NEXT: ret
// CHECK: phi [{{bb[0-9]+}}: {{v[0-9]+}}], [{{bb[0-9]+}}: {{v[0-9]+}}]
// CHECK-NEXT: ret

contract Test {
    function splitUntilZero(uint256[] memory a, uint256[] memory lo, uint256[] memory hi)
        external
        pure
        returns (uint256, uint256[] memory, uint256[] memory)
    {
        uint256 n = _splitUntilZero(a, lo, hi);
        return (n, lo, hi);
    }

    // The high halves are a view into `lo` whose length word is `lo[1]`, which
    // the low stores change: the test fails and the original loop runs.
    function overlappingSplit(uint256[] memory a, uint256[] memory lo)
        external
        pure
        returns (uint256, uint256[] memory, uint256[] memory)
    {
        uint256[] memory hi;
        assembly ("memory-safe") {
            hi := add(lo, 0x40)
        }
        uint256 n = _splitUntilZero(a, lo, hi);
        return (n, lo, hi);
    }

    function _splitUntilZero(uint256[] memory a, uint256[] memory lo, uint256[] memory hi)
        private
        pure
        returns (uint256)
    {
        for (uint256 i; i < a.length; ++i) {
            uint256 v = a[i];
            if (v == 0) return i;
            lo[i] = uint128(v);
            hi[i] = v >> 128;
        }
        return a.length;
    }
}
