//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: merge [1, 3, 5, 7], [2, 3, 8] => [1, 2, 3, 5, 7, 8]
//@ run-call: merge [], [4, 9] => [4, 9]
//@ run-call: merge [6], [] => [6]
//@ run-call: merge [2, 4], [2, 4] => [2, 4]

// A merge steps `i` on the arms that consume `a[i]` and `j` on the arms that
// consume `b[j]`, so neither counter is stepped on the latch alone.
//
// NOTE: both counters should walk their input as carried pointers, and do not
// on this base. #1505 dropped the rule that a memory-object parameter was
// allocated before the function ran, so `a.length` and `b.length` are reloaded
// in the header; an exit test against a reloaded length is not invariant, the
// counters are not free, and the reduction cannot pay for itself. See the note
// in `codegen/core/bytes_write_alias.sol`.
// CHECK-LABEL: fn @merge
// CHECK: {{v[0-9]+}} = phi [bb{{[0-9]+}}: 0], [bb{{[0-9]+}}: {{v[0-9]+}}]
// CHECK-NEXT: {{v[0-9]+}} = phi
// CHECK-NEXT: {{v[0-9]+}} = phi
// CHECK: [[EQ:v[0-9]+]] = eq {{v[0-9]+}}, {{v[0-9]+}}
// CHECK-NEXT: jumpi [[EQ]]

contract Test {
    function merge(uint256[] memory a, uint256[] memory b)
        external
        pure
        returns (uint256[] memory c)
    {
        c = new uint256[](a.length + b.length);
        uint256 i;
        uint256 j;
        uint256 k;
        while (i < a.length && j < b.length) {
            uint256 u = a[i];
            uint256 v = b[j];
            if (u == v) {
                c[k] = u;
                ++k;
                ++i;
                ++j;
            } else if (u > v) {
                c[k] = v;
                ++k;
                ++j;
            } else {
                c[k] = u;
                ++k;
                ++i;
            }
        }
        while (i < a.length) {
            c[k] = a[i];
            ++k;
            ++i;
        }
        while (j < b.length) {
            c[k] = b[j];
            ++k;
            ++j;
        }
        assembly {
            mstore(c, k)
        }
    }
}
