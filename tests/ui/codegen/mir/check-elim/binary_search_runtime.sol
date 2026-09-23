//@ codegen-matrix: standard
//@ run-call: search [], 5 => false, 0
//@ run-call: search [7], 7 => true, 0
//@ run-call: search [7], 3 => false, 0
//@ run-call: search [7], 9 => false, 0
//@ run-call: search [1, 3, 5, 7, 9], 7 => true, 3
//@ run-call: search [1, 3, 5, 7, 9], 8 => false, 3
//@ run-call: search [1, 3, 5, 7, 9], 0 => false, 0
//@ run-call: search [1, 3, 5, 7, 9], 10 => false, 4
//@ run-call: search [2, 2, 2, 2], 2 => true, 1
//@ run-call: search [9, 1, 8, 2], 2 => false, 1
//@ run-call: searchSigned [-5, -1, 0, 4], -1 => true, 1
//@ run-call: searchSigned [-5, -1, 0, 4], -3 => false, 0
//@ run-call-fail: forged 115792089237316195423570985008687907853269984665640564039457584007913129639935, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// The search's bounds stay within the array without any assembly in the
// contract, so its checks are proved rather than executed.
contract Search {
    function search(uint256[] memory a, uint256 needle) external pure returns (bool, uint256) {
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
        return (false, h == 0 ? 0 : h - 1);
    }

    function searchSigned(int256[] memory a, int256 needle) external pure returns (bool, uint256) {
        uint256 l = 1;
        uint256 h = a.length;
        while (l <= h) {
            uint256 m = (l + h) / 2;
            int256 t = a[m - 1];
            if (t == needle) return (true, m - 1);
            if (needle <= t) {
                h = m - 1;
            } else {
                l = m + 1;
            }
        }
        return (false, h == 0 ? 0 : h - 1);
    }
}

// Assembly writes the length here, so `l + h` can wrap and must still panic.
contract ForgedSearch {
    function forged(uint256 length, uint256 needle) external pure returns (bool, uint256) {
        uint256[] memory a = new uint256[](0);
        assembly {
            mstore(a, length)
        }
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
        return (false, h == 0 ? 0 : h - 1);
    }
}
