// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";
import {ArithmeticBench} from "../src/ArithmeticBench.sol";

contract ArithmeticBenchTest is Test {
    ArithmeticBench bench;

    function setUp() public {
        bench = new ArithmeticBench();
    }

    function testAddZero() public view {
        assertEq(bench.addZero(42), 42);
    }

    function testSubZero() public view {
        assertEq(bench.subZero(42), 42);
    }

    function testMulOne() public view {
        assertEq(bench.mulOne(42), 42);
    }

    function testDivOne() public view {
        assertEq(bench.divOne(42), 42);
    }

    function testMulZero() public view {
        assertEq(bench.mulZero(42), 0);
    }

    function testConstExpr() public view {
        assertEq(bench.constExpr(), 60);
    }

    function testComplexConstExpr() public view {
        assertEq(bench.complexConstExpr(), 60);
    }

    function testOrZero() public view {
        assertEq(bench.orZero(42), 42);
    }

    function testAndAllOnes() public view {
        assertEq(bench.andAllOnes(42), 42);
    }

    function testXorZero() public view {
        assertEq(bench.xorZero(42), 42);
    }

    function testShlZero() public view {
        assertEq(bench.shlZero(42), 42);
    }

    function testShrZero() public view {
        assertEq(bench.shrZero(42), 42);
    }

    function testMulPow2() public view {
        assertEq(bench.mulPow2(10), 80);
    }

    function testDivPow2() public view {
        assertEq(bench.divPow2(100), 25);
    }

    function testDoubleNot() public view {
        assertEq(bench.doubleNot(42), 42);
    }

    function testChainedIdentities() public view {
        assertEq(bench.chainedIdentities(42), 42);
    }

    function testMixedExpr() public view {
        assertEq(bench.mixedExpr(5, 7), 42);  // 5 + 30 + 7 = 42
    }
}
