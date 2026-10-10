//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: hasDuplicate [0x10000000000000000000000000000000000000001, 1] => true
//@ run-call: hasDuplicate [0x10000000000000000000000000000000000000001, 2] => false
//@ run-call: hasDuplicate [0x10000000000000000000000000000000000000001, 3, 5, 7, 9, 11, 13, 1] => true
//@ run-call: hasDuplicate [0x10000000000000000000000000000000000000001, 3, 5, 7, 9, 11, 13, 2] => false
//@ run-call: sort [0x10000000000000000000000000000000000000002, 3, 1] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003]
//@ run-call: sort [5, 0x10000000000000000000000000000000000000004, 3, 2, 1] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000005]
//@ run-call: sort [20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 0x10000000000000000000000000000000000000005, 4, 3, 2, 1] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000005, 0x0000000000000000000000000000000000000006, 0x0000000000000000000000000000000000000007, 0x0000000000000000000000000000000000000008, 0x0000000000000000000000000000000000000009, 0x000000000000000000000000000000000000000a, 0x000000000000000000000000000000000000000b, 0x000000000000000000000000000000000000000c, 0x000000000000000000000000000000000000000d, 0x000000000000000000000000000000000000000e, 0x000000000000000000000000000000000000000f, 0x0000000000000000000000000000000000000010, 0x0000000000000000000000000000000000000011, 0x0000000000000000000000000000000000000012, 0x0000000000000000000000000000000000000013, 0x0000000000000000000000000000000000000014]
//@ run-call: sort [7, 20, 3, 0x10000000000000000000000000000000000000011, 9, 14, 1, 18, 5, 12, 16, 2, 10, 19, 4, 13, 8, 6, 15, 11] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000005, 0x0000000000000000000000000000000000000006, 0x0000000000000000000000000000000000000007, 0x0000000000000000000000000000000000000008, 0x0000000000000000000000000000000000000009, 0x000000000000000000000000000000000000000a, 0x000000000000000000000000000000000000000b, 0x000000000000000000000000000000000000000c, 0x000000000000000000000000000000000000000d, 0x000000000000000000000000000000000000000e, 0x000000000000000000000000000000000000000f, 0x0000000000000000000000000000000000000010, 0x0000000000000000000000000000000000000011, 0x0000000000000000000000000000000000000012, 0x0000000000000000000000000000000000000013, 0x0000000000000000000000000000000000000014]
//@ run-call: uniquifySorted [1, 0x10000000000000000000000000000000000000001, 2] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002]
//@ run-call: union [0x10000000000000000000000000000000000000001, 3], [1, 2] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003]
//@ run-call: intersection [0x10000000000000000000000000000000000000001, 3], [1, 2] => [0x0000000000000000000000000000000000000001]
//@ run-call: difference [0x10000000000000000000000000000000000000001, 3], [1, 2] => [0x0000000000000000000000000000000000000003]
//@ run-call: groupSum [2, 0x10000000000000000000000000000000000000001, 1], [10, 20, 30] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002], [50, 10]
//@ run-call: copyWords [0x10000000000000000000000000000000000000001, 2] => [1, 2]
//@ run-call: sortWords [1, 0x10000000000000000000000000000000000000002, 3] => [1, 2, 3]
//@ run-call: sortWords [3, 0x10000000000000000000000000000000000000002, 1] => [1, 2, 3]
//@ run-call: unionWords [1, 3], [2, 0x10000000000000000000000000000000000000004] => [1, 2, 3, 4]
//@ run-call: differenceWords [1, 0x10000000000000000000000000000000000000005], [2] => [1, 5]
//@ run-call: groupSumWords [9, 0x10000000000000000000000000000000000000005], [1, 2] => [5, 9]
import {WordArrays} from "solar:core/WordArrays.sol";

// Inline assembly can leave the upper bits of an `address[]` element dirty.
// The module's bodies compare the addresses the elements hold, as their
// typed reads clean them, and the intrinsics must agree. The bodies also
// store the elements they assign through their type, clean, which the words
// the `*Words` functions return show without the ABI's own cleaning.
contract DirtyAddresses {
    function addresses(uint256[] memory words) internal pure returns (address[] memory a) {
        assembly {
            a := words
        }
    }

    function hasDuplicate(uint256[] memory words) external pure returns (bool) {
        return WordArrays.hasDuplicate(addresses(words));
    }

    function sort(uint256[] memory words) external pure returns (address[] memory a) {
        a = addresses(words);
        WordArrays.sort(a);
    }

    function uniquifySorted(uint256[] memory words) external pure returns (address[] memory a) {
        a = addresses(words);
        WordArrays.uniquifySorted(a);
    }

    function union(uint256[] memory a, uint256[] memory b) external pure returns (address[] memory) {
        return WordArrays.union(addresses(a), addresses(b));
    }

    function intersection(uint256[] memory a, uint256[] memory b)
        external
        pure
        returns (address[] memory)
    {
        return WordArrays.intersection(addresses(a), addresses(b));
    }

    function difference(uint256[] memory a, uint256[] memory b)
        external
        pure
        returns (address[] memory)
    {
        return WordArrays.difference(addresses(a), addresses(b));
    }

    function groupSum(uint256[] memory words, uint256[] memory values)
        external
        pure
        returns (address[] memory keys, uint256[] memory)
    {
        keys = addresses(words);
        WordArrays.groupSum(keys, values);
        return (keys, values);
    }

    function words(address[] memory a) internal pure returns (uint256[] memory w) {
        assembly {
            w := a
        }
    }

    function copyWords(uint256[] memory a) external pure returns (uint256[] memory) {
        return words(WordArrays.copy(addresses(a)));
    }

    function sortWords(uint256[] memory a) external pure returns (uint256[] memory) {
        address[] memory sorted = addresses(a);
        WordArrays.sort(sorted);
        return words(sorted);
    }

    function unionWords(uint256[] memory a, uint256[] memory b)
        external
        pure
        returns (uint256[] memory)
    {
        return words(WordArrays.union(addresses(a), addresses(b)));
    }

    function differenceWords(uint256[] memory a, uint256[] memory b)
        external
        pure
        returns (uint256[] memory)
    {
        return words(WordArrays.difference(addresses(a), addresses(b)));
    }

    function groupSumWords(uint256[] memory a, uint256[] memory values)
        external
        pure
        returns (uint256[] memory)
    {
        address[] memory keys = addresses(a);
        WordArrays.groupSum(keys, values);
        return words(keys);
    }
}
