// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/index_multiply.sol";

contract IndexMultiplyTest {
    IndexMultiply c;

    function setUp() public {
        c = new IndexMultiply();
    }

    function test_ComputeOffsets() public view {
        uint256[] memory r = c.computeOffsets(5);
        assert(r.length == 5);
        for (uint256 i = 0; i < 5; i++) {
            assert(r[i] == i * 32);
        }
    }

    function test_ComputeOffsetsOptimizedMatches() public view {
        uint256[] memory a = c.computeOffsets(7);
        uint256[] memory b = c.computeOffsetsOptimized(7);
        assert(a.length == b.length);
        for (uint256 i = 0; i < a.length; i++) {
            assert(a[i] == b[i]);
        }
    }

    function test_ComputeSlots() public view {
        uint256[] memory r = c.computeSlots(5);
        assert(r.length == 5);
        for (uint256 i = 0; i < 5; i++) {
            assert(r[i] == i * 0x20);
        }
    }

    function test_MultiplePatterns() public view {
        (uint256 sum1, uint256 sum2) = c.multiplePatterns(4);
        // 5 * (0 + 1 + 2 + 3) and 10 * (0 + 1 + 2 + 3).
        assert(sum1 == 30);
        assert(sum2 == 60);
    }

    function test_ZeroCount() public view {
        assert(c.computeOffsets(0).length == 0);
        (uint256 sum1, uint256 sum2) = c.multiplePatterns(0);
        assert(sum1 == 0 && sum2 == 0);
    }
}
