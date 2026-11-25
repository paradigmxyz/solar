//@ codegen-matrix: standard
//@ run-call: counter 5, 3 => [5, 0x80000000000000000000000000000006, 0x100000000000000000000000000000007]
//@ run-call: offset 5, 2 => [0x80000000000000000000000000000006, 0x100000000000000000000000000000007]
//@ run-call: invariant 5, 1, 2 => [0x80000000000000000000000000000006, 0x100000000000000000000000000000007]
//@ run-call: negated 0x100000000000000000000000000000000, 3 => [0x100000000000000000000000000000000, 0x80000000000000000000000000000000, 0]
//@ run-call: negatedInvariant 5, 3, 2 => [0xfffffffffffffffffffffffffffffffe80000000000000000000000000000005, 0xfffffffffffffffffffffffffffffffe80000000000000000000000000000025]
//@ run-call: negatedOffset 5, 2 => [0xffffffffffffffffffffffffffffffff80000000000000000000000000000005, 0xffffffffffffffffffffffffffffffff80000000000000000000000000000007], [5, 7]
// A shift by 127 scales its operand by 2^127, one more than the largest `i128`. Read as an
// `i128` it is -2^127, which would give the counter, a constant, or a loop-invariant value
// inside the shift the wrong sign in the affine form. `offset` and `invariant` go wrong only
// without overflow checks, so they use `unchecked`. The `negated` functions build a scale or
// a constant that really is -2^127, whose `abs` used to panic in a debug build.
contract ShiftScaleSign {
    function counter(uint256 base, uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            out[i] = base + (i << 127) + i;
        }
    }

    function offset(uint256 base, uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        unchecked {
            for (uint256 i; i < n; ++i) {
                out[i] = base + ((i + 1) << 127) + i + 1;
            }
        }
    }

    function invariant(uint256 base, uint256 x, uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        unchecked {
            for (uint256 i; i < n; ++i) {
                out[i] = base + ((i + x) << 127) + i + x;
            }
        }
    }

    function negated(uint256 base, uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        unchecked {
            for (uint256 i; i < n; ++i) {
                out[i] = base - (i << 126) - (i << 126);
            }
        }
    }

    function negatedInvariant(uint256 base, uint256 x, uint256 n) external pure returns (uint256[] memory out) {
        out = new uint256[](n);
        unchecked {
            for (uint256 i; i < n; ++i) {
                out[i] = base - (x << 126) - (x << 126) + (i << 5);
            }
        }
    }

    function negatedOffset(uint256 base, uint256 n) external pure returns (uint256[] memory out, uint256[] memory out2) {
        out = new uint256[](n);
        out2 = new uint256[](n);
        unchecked {
            for (uint256 i; i < n; ++i) {
                out[i] = base + ((i - (1 << 126)) << 1);
                out2[i] = base + (i << 1);
            }
        }
    }
}
