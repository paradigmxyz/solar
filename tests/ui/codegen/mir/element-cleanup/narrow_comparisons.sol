//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck:
//@[opt] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: search [1, 3, 5], 3 => 1
//@ run-call: search [1, 3, 5], 6 => 3
//@ run-call: search [], 4 => 0
//@ run-call: union [1, 3, 8], [2, 3, 9] => [1, 2, 3, 8, 9]
//@ run-call: union [], [7] => [7]
//@ run-call: shrink [1, 2], 1 => [1]
//@ run-call-fail: shrink [1, 2], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: dirty [1, 1], 0, 0x10000000000000000000000000000000000000001 => true
//@ run-call: below [5], 0x10000000000000000000000000000000000000001 => false

// An unsigned comparison of `uint160` elements whose arrays only ever hold
// canonical words compares the loaded words, so neither the search nor the
// merge masks its elements. Results built at their largest size and shrunk
// in place keep canonical elements and encode without a copy.
contract NarrowComparisons {
    // CHECK-LABEL: fn @search(
    // CHECK-NOT: 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: returndata
    function search(uint160[] memory a, uint160 x) external pure returns (uint256 low) {
        uint256 high = a.length;
        while (low < high) {
            uint256 mid = (low + high) >> 1;
            if (a[mid] < x) low = mid + 1;
            else high = mid;
        }
    }

    // CHECK-LABEL: fn @union(
    // CHECK-NOT: 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK-NOT: mcopy
    // CHECK: returndata
    function union(uint160[] memory a, uint160[] memory b) external pure returns (uint160[] memory c) {
        c = new uint160[](a.length + b.length);
        uint256 i;
        uint256 j;
        uint256 k;
        while (i < a.length && j < b.length) {
            uint160 u = a[i];
            uint160 v = b[j];
            if (u == v) {
                c[k++] = u;
                ++i;
                ++j;
            } else if (u > v) {
                c[k++] = v;
                ++j;
            } else {
                c[k++] = u;
                ++i;
            }
        }
        while (i < a.length) c[k++] = a[i++];
        while (j < b.length) c[k++] = b[j++];
        _truncate(c, k);
    }

    // CHECK-LABEL: fn @shrink(
    // CHECK-NOT: 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK-NOT: mcopy
    // CHECK: returndata
    function shrink(uint160[] memory a, uint256 n) external pure returns (uint160[] memory) {
        _truncate(a, n);
        return a;
    }

    // A word assembly may have stored dirty keeps the mask on the comparison.
    // CHECK-LABEL: fn @dirty(
    // CHECK: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function dirty(uint160[] memory a, uint256 i, uint256 x) external pure returns (bool) {
        assembly {
            mstore(add(add(a, 0x20), shl(5, i)), x)
        }
        return a[0] == a[1];
    }

    // The other operand is zero-extended, which leaves its own conversion in place.
    // CHECK-LABEL: fn @below(
    // CHECK: [[ELEMENT:v[0-9]+]] = add {{v[0-9]+}}, 32
    // CHECK-NEXT: [[WORD:v[0-9]+]] = mload [[ELEMENT]]
    // CHECK-NEXT: [[X:v[0-9]+]] = and {{arg[0-9]+|v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK-NEXT: {{v[0-9]+}} = lt [[WORD]], [[X]]
    function below(uint160[] memory a, uint256 x) external pure returns (bool) {
        return a[0] < uint160(x);
    }

    /// Shrinks `a` to `n` elements, which a checked length bounds.
    function _truncate(uint160[] memory a, uint256 n) private pure {
        if (n > a.length) n = _outOfBounds();
        assembly ("memory-safe") {
            mstore(a, n)
        }
    }

    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
