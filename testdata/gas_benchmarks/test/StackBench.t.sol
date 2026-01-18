// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";
import {StackBench} from "../src/StackBench.sol";

contract StackBenchTest is Test {
    StackBench bench;

    function setUp() public {
        bench = new StackBench();
    }

    function testValueReuse() public view {
        uint256 result = bench.valueReuse(5);
        assertEq(result, 20);  // 5 + 5 + 5 + 5
    }

    function testDeepStack() public view {
        uint256 result = bench.deepStack(1, 2, 3, 4, 5, 6, 7, 8);
        assertEq(result, 36);  // 1+8+2+7+3+6+4+5 = 36
    }

    function testTempValues() public view {
        uint256 result = bench.tempValues(2, 3, 4);
        // t1 = 2+3 = 5, t2 = 3+4 = 7, t3 = 2+4 = 6 (unused)
        assertEq(result, 35);  // 5 * 7 = 35
    }

    function testNestedExpr() public view {
        uint256 result = bench.nestedExpr(10, 4, 6, 2);
        // ((10+4) * (6+2)) + ((10-4) * (6-2))
        // = (14 * 8) + (6 * 4)
        // = 112 + 24 = 136
        assertEq(result, 136);
    }

    function testMultiReturn() public view {
        (uint256 sum, uint256 diff, uint256 prod) = bench.multiReturn(10, 3);
        assertEq(sum, 13);
        assertEq(diff, 7);
        assertEq(prod, 30);
    }

    function testManyLocals() public view {
        uint256 result = bench.manyLocals(1);
        // v1=2, v2=3, v3=4, v4=5, v5=6, v6=7, v7=8, v8=9
        // return v1 + v4 + v8 = 2 + 5 + 9 = 16
        assertEq(result, 16);
    }

    function testComplexStack() public view {
        uint256 result = bench.complexStack(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
        // r1 = 1+2+3 = 6
        // r2 = 4+5+6 = 15
        // r3 = 7+8+9+10 = 34
        // total = 55
        assertEq(result, 55);
    }
}
