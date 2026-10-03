//@ codegen-matrix: standard
//@ run-call: sum [] => 0
//@ run-call: sum [1, 2, 3] => 6
//@ run-call: tail [] => 0
//@ run-call: tail [5] => 0
//@ run-call: tail [5, 6, 7] => 13
//@ run-call: scan [1, 2, 3], 0 => 0
//@ run-call: scan [1, 2, 3], 2 => 3
//@ run-call: scan [1, 2, 3], 3 => 6
//@ run-call-fail: scan [1, 2, 3], 4
//@ run-call-fail: scan [1, 2, 3], 0x0800000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: scan [1, 2, 3], 0x0800000000000000000000000000000000000000000000000000000000000002
//@ run-call: scanFrom1 [1, 2, 3], 0 => 0
//@ run-call: scanFrom1 [1, 2, 3], 1 => 0
//@ run-call: scanFrom1 [1, 2, 3], 3 => 5
//@ run-call-fail: scanFrom1 [1, 2, 3], 0x0800000000000000000000000000000000000000000000000000000000000001
//@ run-call: find [4, 5, 6], 4 => 0
//@ run-call: find [4, 5, 6], 6 => 2
//@ run-call: find [4, 5, 6], 7 => 3
//@ run-call: find [], 7 => 0
//@ run-call: findFrom1 [4, 5, 6], 4 => 3
//@ run-call: findFrom1 [4, 5, 6], 5 => 1
//@ run-call: findFrom1 [4, 5, 6], 6 => 2
//@ run-call: findFrom1 [], 5 => 0
//@ run-call: stopAt [4, 5, 6], 5 => 1
//@ run-call: stopAt [4, 5, 6], 9 => 3
//@ run-call: stopAt [], 9 => 0
pragma solidity ^0.8.0;

// A calldata pointer may wrap, so it leaves a loop by reaching its value at the bound
// rather than passing it. The decoded length is checked against the calldata size, which
// keeps it far enough below the word size; a bound from the caller is clamped first, or
// `2^251` elements of 32 bytes would wrap the end back onto the start. A search that
// returns where it stopped rebuilds that index from the pointer after the loop.
contract CalldataPointerExit {
    function sum(uint256[] calldata xs) external pure returns (uint256 total) {
        for (uint256 i = 0; i < xs.length; ++i) {
            total += xs[i];
        }
    }

    function tail(uint256[] calldata xs) external pure returns (uint256 total) {
        for (uint256 i = 1; i < xs.length; ++i) {
            total += xs[i];
        }
    }

    function scan(uint256[] calldata xs, uint256 n) external pure returns (uint256 total) {
        for (uint256 i = 0; i < n; ++i) {
            uint256 x;
            assembly {
                x := calldataload(add(xs.offset, shl(5, i)))
            }
            require(x != 0);
            total += x;
        }
    }

    function scanFrom1(uint256[] calldata xs, uint256 n) external pure returns (uint256 total) {
        for (uint256 i = 1; i < n; ++i) {
            uint256 x;
            assembly {
                x := calldataload(add(xs.offset, shl(5, i)))
            }
            require(x != 0);
            total += x;
        }
    }

    function find(uint256[] calldata xs, uint256 x) external pure returns (uint256 found) {
        found = xs.length;
        for (uint256 i = 0; i < xs.length; ++i) {
            if (xs[i] == x) {
                found = i;
                break;
            }
        }
    }

    function findFrom1(uint256[] calldata xs, uint256 x) external pure returns (uint256 found) {
        found = xs.length;
        for (uint256 i = 1; i < xs.length; ++i) {
            if (xs[i] == x) {
                found = i;
                break;
            }
        }
    }

    function stopAt(uint256[] calldata xs, uint256 x) external pure returns (uint256) {
        uint256 i;
        for (; i < xs.length; ++i) {
            if (xs[i] == x) break;
        }
        return i;
    }
}
