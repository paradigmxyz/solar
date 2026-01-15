// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Showcase.sol";

contract ShowcaseTest {
    Showcase public showcase;

    function setUp() public {
        showcase = new Showcase();
    }

    function test_UltimateStressTest() public view {
        uint256 result = showcase.ultimateStressTest(
            1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16
        );
        assert(result > 0);
    }

    function test_CalculateSwap() public view {
        (uint256 amountOut, uint256 priceImpact, uint256 effectiveFee) = showcase.calculateSwap(
            1000000e18,  // reserve0
            1000000e18,  // reserve1
            30,          // 0.3% fee
            1000e18,     // amountIn
            100,         // 1% slippage tolerance
            900e18,      // minOut
            1100e18,     // maxOut
            2e18         // priceLimit
        );
        assert(amountOut > 0);
    }

    function test_BatchSum() public view {
        // sum1=3, sum2=7, sum3=11, sum4=15, sum5=19 => total=55
        // prod1=2, prod2=12, prod3=30, prod4=56, prod5=90 => prodTotal=190
        // inputs sum = 1+2+3+4+5+6+7+8+9+10 = 55
        // expected = 55 + 190 + 55 = 300
        uint256 result = showcase.batchSum(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
        assert(result == 300);
    }

    function test_ChainedArithmetic() public view {
        uint256 result = showcase.chainedArithmetic(1, 2, 3, 4, 5, 6, 7, 8);
        assert(result > 0);
    }

    function test_SimpleAdd() public view {
        uint256 result = showcase.simpleAdd(40, 2);
        assert(result == 42);
    }
}
