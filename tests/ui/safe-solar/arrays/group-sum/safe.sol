//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: groupUint [3, 1, 3, 2, 1], [10, 20, 30, 40, 50] => [1, 2, 3], [70, 40, 40]
//@ run-call: groupUint [], [] => [], []
//@ run-call: groupUint [5], [7] => [5], [7]
//@ run-call: groupUint [4, 4, 4], [1, 2, 3] => [4], [6]
//@ run-call: groupUint [9, 8, 7, 6], [1, 2, 3, 4] => [6, 7, 8, 9], [4, 3, 2, 1]
//@ run-call-fail: groupUint [1, 2], [1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: groupUint [1], [] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: groupUint [1, 1], [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: groupInt [-1, 5, -1, 0], [1, 2, 3, 4] => [0, 5, -1], [4, 2, 4]
//@ run-call: groupAddress [0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000003], [1, 2, 3] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000003], [2, 4]
//@ run-call: groupBytes32 [0x0000000000000000000000000000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000000000000000000000000000001], [5, 6] => [0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000002], [6, 5]
//@ run-call: groupGenerated 257, 7; gas=15000000 => true
//@ run-call: groupGenerated 40, 3; gas=15000000 => true
//@ run-call: groupDescending 64; gas=15000000 => true
//@ run-call: groupAliased 9, 5, false; gas=15000000 => true
//@ run-call: groupAliased 57, 5, false; gas=15000000 => true
//@ run-call: groupAliased 40, 0, true; gas=15000000 => true

// `groupSum` orders keys by their word value, keeps one of each, and gives
// it the sum of the values its copies had. Both arrays shrink to the kept
// keys; `int256` keys order as words, so negative keys come last.

import {WordArrays} from "solar:core/v1/WordArrays.sol";

contract Safe {
    function groupUint(uint256[] memory keys, uint256[] memory values)
        public
        pure
        returns (uint256[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    function groupInt(int256[] memory keys, uint256[] memory values)
        public
        pure
        returns (int256[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    function groupAddress(address[] memory keys, uint256[] memory values)
        public
        pure
        returns (address[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    function groupBytes32(bytes32[] memory keys, uint256[] memory values)
        public
        pure
        returns (bytes32[] memory, uint256[] memory)
    {
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    // Enough keys for quicksort partitions, checked against per-key totals.
    function groupGenerated(uint256 n, uint256 seed) public pure returns (bool) {
        uint256[] memory keys = new uint256[](n);
        uint256[] memory values = new uint256[](n);
        uint256[] memory totals = new uint256[](23);
        for (uint256 i; i < n; ++i) {
            keys[i] = uint256(keccak256(abi.encode(seed, i))) % 23;
            values[i] = i + 1;
            totals[keys[i]] += i + 1;
        }
        WordArrays.groupSum(keys, values);
        uint256 distinct;
        for (uint256 k; k < 23; ++k) if (totals[k] != 0) ++distinct;
        if (keys.length != distinct || values.length != distinct) return false;
        for (uint256 i; i < keys.length; ++i) {
            if (i != 0 && keys[i - 1] >= keys[i]) return false;
            if (values[i] != totals[keys[i]]) return false;
        }
        return true;
    }

    // Keys and values in one array: the words sort as keys, then each kept
    // slot takes its run's sum, as the reference below computes in place.
    function groupAliased(uint256 n, uint256 seed, bool descending) public pure returns (bool) {
        uint256[] memory a = new uint256[](n);
        uint256[] memory expected = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = descending ? n - i : uint256(keccak256(abi.encode(seed, i))) % 7;
            expected[i] = a[i];
        }
        for (uint256 i = 1; i < n; ++i) {
            uint256 key = expected[i];
            uint256 j = i;
            while (j != 0 && expected[j - 1] > key) {
                expected[j] = expected[j - 1];
                --j;
            }
            expected[j] = key;
        }
        uint256 count = n;
        if (n > 1) {
            uint256 kept;
            uint256 sum = expected[0];
            for (uint256 i = 1; i < n; ++i) {
                if (expected[i] == expected[kept]) {
                    sum += expected[i];
                } else {
                    expected[kept] = sum;
                    expected[++kept] = expected[i];
                    sum = expected[i];
                }
            }
            expected[kept] = sum;
            count = kept + 1;
        }
        WordArrays.groupSum(a, a);
        if (a.length != count) return false;
        for (uint256 i; i < count; ++i) {
            if (a[i] != expected[i]) return false;
        }
        return true;
    }

    // A strictly descending run takes the reversal path.
    function groupDescending(uint256 n) public pure returns (bool) {
        uint256[] memory keys = new uint256[](n);
        uint256[] memory values = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            keys[i] = n - i;
            values[i] = i;
        }
        WordArrays.groupSum(keys, values);
        for (uint256 i; i < n; ++i) {
            if (keys[i] != i + 1 || values[i] != n - 1 - i) return false;
        }
        return true;
    }
}
