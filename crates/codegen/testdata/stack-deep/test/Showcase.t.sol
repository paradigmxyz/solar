// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Showcase.sol";

interface Vm { function envBytes(string calldata) external view returns (bytes memory); }

contract ShowcaseTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Showcase public showcase;

    function _deploy(string memory n) internal returns (address d) {
        try vm.envBytes(string.concat("SOLAR_", n, "_BYTECODE")) returns (bytes memory c) {
            assembly { d := create(0, add(c, 0x20), mload(c)) }
        } catch { d = address(new Showcase()); }
    }

    function setUp() public {
        showcase = Showcase(_deploy("SHOWCASE"));
    }

    function test_UltimateStressTest() public view {
        uint256 result = showcase.ultimateStressTest(
            1, 2, 3, 4, 5, 6, 7, 8,
            9, 10, 11, 12, 13, 14, 15, 16
        );
        require(result > 0, "should return positive");
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
        require(amountOut > 0, "no output");
    }

    function test_BatchSum() public view {
        // sum1=3, sum2=7, sum3=11, sum4=15, sum5=19 => total=55
        // prod1=2, prod2=12, prod3=30, prod4=56, prod5=90 => prodTotal=190
        // inputs sum = 1+2+3+4+5+6+7+8+9+10 = 55
        // expected = 55 + 190 + 55 = 300
        uint256 result = showcase.batchSum(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
        require(result == 300, "wrong result");
    }

    function test_ChainedArithmetic() public view {
        uint256 result = showcase.chainedArithmetic(1, 2, 3, 4, 5, 6, 7, 8);
        require(result > 0, "should return positive");
    }

    function test_SimpleAdd() public view {
        uint256 result = showcase.simpleAdd(40, 2);
        require(result == 42, "should be 42");
    }
}
