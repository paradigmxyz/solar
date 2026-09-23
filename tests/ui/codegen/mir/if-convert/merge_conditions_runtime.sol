//@ codegen-matrix: standard
//@ run-call: search [1, 3, 5, 7, 9], 7 => true, 3
//@ run-call: search [1, 3, 5, 7, 9], 4 => false, 1
//@ run-call: search [1, 3, 5, 7, 9], 0 => false, 0
//@ run-call: search [1, 3, 5, 7, 9], 10 => false, 4
//@ run-call: search [], 3 => false, 0
//@ run-call: search [5, 1], 5 => true, 0
//@ run-call: countBoth [1, 5, 7, 9], 4, 7 => 2
//@ run-call: countBoth [], 4, 7 => 0
//@ run-call: countEither [1, 5, 7, 9], 4, 7 => 2
//@ run-call: countEither [8, 9], 4, 7 => 0

// Short-circuit tests with pure operands merge into one branch. Each result
// must match short-circuit evaluation, including the search's final probe
// after `l > h`, which an unsorted array can observe.

contract MergeConditions {
    function search(uint256[] memory a, uint256 needle)
        external
        pure
        returns (bool found, uint256 index)
    {
        uint256 l = 1;
        uint256 h = a.length;
        uint256 t;
        while (true) {
            index = (l + h) / 2;
            if (index != 0) t = a[index - 1];
            if (l > h || (index != 0 && t == needle)) break;
            if (needle <= t) {
                h = index - 1;
            } else {
                l = index + 1;
            }
        }
        found = index != 0 && t == needle;
        if (index != 0) index -= 1;
    }

    function countBoth(uint256[] memory a, uint256 low, uint256 excluded)
        external
        pure
        returns (uint256 count)
    {
        for (uint256 i; i < a.length; ++i) {
            uint256 x = a[i];
            if (x > low && x != excluded) ++count;
        }
    }

    function countEither(uint256[] memory a, uint256 low, uint256 included)
        external
        pure
        returns (uint256 count)
    {
        for (uint256 i; i < a.length; ++i) {
            uint256 x = a[i];
            if (x < low || x == included) ++count;
        }
    }
}
