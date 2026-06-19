// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ControlFlow.sol";

contract ControlFlowTest {
    ControlFlow cf;

    function setUp() public {
        cf = new ControlFlow();
    }

    // ========== Conditionals ==========

    function test_SimpleIfTrue() public view {
        assert(cf.simpleIf(15) == 1);
    }

    function test_SimpleIfFalse() public view {
        assert(cf.simpleIf(5) == 0);
    }

    function test_SimpleIfBoundary() public view {
        assert(cf.simpleIf(10) == 0);
        assert(cf.simpleIf(11) == 1);
    }

    function test_IfElse() public view {
        assert(cf.ifElse(15) == 2);
        assert(cf.ifElse(5) == 1);
    }

    function test_IfElseIf() public view {
        assert(cf.ifElseIf(150) == 3);
        assert(cf.ifElseIf(50) == 2);
        assert(cf.ifElseIf(5) == 1);
    }

    function test_NestedIf() public view {
        assert(cf.nestedIf(15, 15) == 4);
        assert(cf.nestedIf(15, 5) == 3);
        assert(cf.nestedIf(5, 15) == 2);
        assert(cf.nestedIf(5, 5) == 1);
    }

    // ========== For Loops ==========

    function test_ForLoopSum() public view {
        assert(cf.forLoopSum(0) == 0);
        assert(cf.forLoopSum(1) == 0);
        assert(cf.forLoopSum(5) == 10);
        assert(cf.forLoopSum(10) == 45);
    }

    function test_ForLoopProduct() public view {
        assert(cf.forLoopProduct(0) == 0);
        assert(cf.forLoopProduct(1) == 1);
        assert(cf.forLoopProduct(5) == 120);
    }

    function test_NestedForLoop() public view {
        assert(cf.nestedForLoop(3, 4) == 12);
        assert(cf.nestedForLoop(0, 5) == 0);
        assert(cf.nestedForLoop(5, 0) == 0);
        assert(cf.nestedForLoop(1, 1) == 1);
    }

    // ========== While Loops ==========

    function test_WhileLoopSum() public view {
        assert(cf.whileLoopSum(5) == 10);
        assert(cf.whileLoopSum(0) == 0);
    }

    // ========== Break and Continue ==========

    function test_ForWithBreak() public view {
        assert(cf.forWithBreak(5) == 10);
        assert(cf.forWithBreak(0) == 0);
        assert(cf.forWithBreak(3) == 3);
    }

    function test_ForWithContinue() public view {
        assert(cf.forWithContinue(10) == 25);
        assert(cf.forWithContinue(5) == 4);
        assert(cf.forWithContinue(1) == 0);
    }

    function test_WhileWithBreak() public view {
        assert(cf.whileWithBreak(5) == 5);
        assert(cf.whileWithBreak(0) == 0);
    }

    function test_NestedLoopBreak() public view {
        assert(cf.nestedLoopBreak(15) == 15);
        assert(cf.nestedLoopBreak(100) == 100);
        assert(cf.nestedLoopBreak(5) == 5);
    }

    // ========== Edge Cases ==========

    function test_ZeroIterations() public view {
        assert(cf.zeroIterations() == 0);
    }

    function test_SingleIteration() public view {
        assert(cf.singleIteration() == 10);
    }

    function test_EarlyReturn() public view {
        assert(cf.earlyReturn(0) == 999);
        assert(cf.earlyReturn(5) == 500);
        assert(cf.earlyReturn(100) == 0);
    }
}
