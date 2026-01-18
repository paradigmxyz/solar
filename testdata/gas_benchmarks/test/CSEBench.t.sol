// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";
import {CSEBench} from "../src/CSEBench.sol";

contract CSEBenchTest is Test {
    CSEBench bench;

    function setUp() public {
        bench = new CSEBench();
    }

    function testDuplicateAdd() public view {
        uint256 result = bench.duplicateAdd(10, 20);
        assertEq(result, 60);  // (10+20) + (10+20) = 60
    }

    function testCommutativeAdd() public view {
        uint256 result = bench.commutativeAdd(5, 7);
        assertEq(result, 144);  // (5+7) * (7+5) = 12 * 12 = 144
    }

    function testComplexCSE() public view {
        uint256 result = bench.complexCSE(3, 4, 2);
        assertEq(result, 24);  // (3*4+2) + (3*4-2) = 14 + 10 = 24
    }

    function testNonCSE() public view {
        uint256 result = bench.nonCSE(1, 2, 3);
        assertEq(result, 8);  // (1+2) + (2+3) = 3 + 5 = 8
    }

    function testMulCSE() public view {
        uint256 result = bench.mulCSE(3, 5);
        assertEq(result, 30);  // 15 + 15 = 30
    }

    function testBitwiseCSE() public view {
        uint256 result = bench.bitwiseCSE(0xFF, 0x0F);
        // (0xFF & 0x0F) + (0xFF & 0x0F) + (0xFF | 0x0F) + (0x0F | 0xFF)
        // = 0x0F + 0x0F + 0xFF + 0xFF = 15 + 15 + 255 + 255 = 540
        assertEq(result, 540);
    }

    function testComparisonCSE() public view {
        assertTrue(bench.comparisonCSE(42, 42));
        assertFalse(bench.comparisonCSE(1, 2));
    }
}
