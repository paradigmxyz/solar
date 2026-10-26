//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: search [], 5 => false, 0
//@ run-call: search [5], 5 => true, 0
//@ run-call: search [5], 4 => false, 0
//@ run-call: search [5], 6 => false, 0
//@ run-call: search [1, 3, 5, 7, 9], 1 => true, 0
//@ run-call: search [1, 3, 5, 7, 9], 9 => true, 4
//@ run-call: search [1, 3, 5, 7, 9], 6 => false, 2
//@ run-call: search [1, 3, 5, 7, 9], 0 => false, 0
//@ run-call: search [1, 3, 5, 7, 9], 10 => false, 4
//@ run-call: lowerBound [], 1 => 0
//@ run-call: lowerBound [1, 3, 5, 7, 9], 0 => 0
//@ run-call: lowerBound [1, 3, 5, 7, 9], 6 => 3
//@ run-call: lowerBound [1, 3, 5, 7, 9], 9 => 4
//@ run-call: lowerBound [1, 3, 5, 7, 9], 10 => 5
//@ run-call: consume [10, 20, 30], [true, false, true, true] => 60, 3
//@ run-call: consume [10], [true, true] => 10, 1
//@ run-call: consume [], [false, true] => 0, 0
//@ run-call: probe [1, 2, 3], 1 => 2
//@ run-call-fail: probe [1, 2, 3], 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A binary search keeps `1 <= l` and `h <= a.length` on every iteration: each
// backedge carries `l`, `h`, or a midpoint step that stays between them. The
// midpoint `m = (l + h) / 2` lies between `l` and `h`, so `m - 1` cannot
// underflow and stays below the length, and with the decoded length's bound
// neither can `l + h` overflow: the loop keeps no check.
// CHECK-LABEL: fn @search(
// CHECK-NOT: {{mstore 32, 50|tail_call @revert_stub}}
// CHECK-NOT: mstore 32, 17
// CHECK: returndata

// The half-open form keeps `hi <= a.length`, and its midpoint
// `lo + (hi - lo) / 2` stays below `hi` while `lo < hi`.
// CHECK-LABEL: fn @lowerBound(
// CHECK-NOT: {{mstore 32, 50|tail_call @revert_stub}}
// CHECK-NOT: mstore 32, 17
// CHECK: returndata

// The cursor only steps after `used != proof.length` and is compared with the
// calldata length, so it stays at most the length; with the inequality, its
// read needs no bounds check.
// CHECK-LABEL: fn @consume(
// CHECK-NOT: {{mstore 32, 50|tail_call @revert_stub}}
// CHECK: returndata

// A probe that moves past the length keeps no bound, so its access keeps its
// check.
// CHECK-LABEL: fn @probe(
// CHECK: {{mstore 32, 50|tail_call @revert_stub}}
contract HeaderBounds {
    function search(uint256[] memory a, uint256 needle) external pure returns (bool found, uint256 index) {
        uint256 l = 1;
        uint256 h = a.length;
        while (l <= h) {
            uint256 m = (l + h) / 2;
            uint256 t = a[m - 1];
            if (t == needle) return (true, m - 1);
            if (needle <= t) {
                h = m - 1;
            } else {
                l = m + 1;
            }
        }
        if (h != 0) index = h - 1;
    }

    function lowerBound(uint256[] memory a, uint256 needle) external pure returns (uint256) {
        uint256 lo;
        uint256 hi = a.length;
        while (lo < hi) {
            uint256 mid = lo + (hi - lo) / 2;
            if (a[mid] < needle) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        return lo;
    }

    function consume(uint256[] calldata proof, bool[] calldata take)
        external
        pure
        returns (uint256 sum, uint256 used)
    {
        for (uint256 i; i < take.length; ++i) {
            if (!take[i]) continue;
            if (used == proof.length) return (sum, used);
            sum += proof[used];
            ++used;
        }
    }

    function probe(uint256[] memory a, uint256 steps) external pure returns (uint256 sum) {
        uint256 l;
        uint256 h = a.length;
        for (uint256 k; k < steps; ++k) {
            uint256 m = (l + h) / 2;
            sum += a[m];
            l = h;
            h = h + 1;
        }
    }
}
