// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/sum_array.sol";

contract SumArrayTest {
    SumArray c;

    function setUp() public {
        c = new SumArray();
    }

    function test_Sum() public view {
        uint256[] memory arr = new uint256[](5);
        for (uint256 i = 0; i < 5; i++) {
            arr[i] = i + 1;
        }
        assert(c.sum(arr) == 15);
        assert(c.sumOptimized(arr) == 15);
    }

    function test_SumEmpty() public view {
        uint256[] memory arr = new uint256[](0);
        assert(c.sum(arr) == 0);
        assert(c.sumOptimized(arr) == 0);
    }
}
