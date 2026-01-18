// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressControlFlow.sol";

contract StressControlFlowTest {
    StressControlFlow scf;
    
    function setUp() public {
        scf = new StressControlFlow();
    }
    
    // ========== Deep if-else tests ==========
    
    function test_DeepIfElse_Level1() public view {
        assert(scf.deepIfElse(50) == 1);
    }
    
    function test_DeepIfElse_Level2() public view {
        assert(scf.deepIfElse(200) == 2);
    }
    
    function test_DeepIfElse_Level3() public view {
        assert(scf.deepIfElse(600) == 3);
    }
    
    function test_DeepIfElse_Level4() public view {
        assert(scf.deepIfElse(920) == 4);
    }
    
    function test_DeepIfElse_Level5() public view {
        assert(scf.deepIfElse(960) == 5);
    }
    
    function test_DeepIfElse_Level10() public view {
        assert(scf.deepIfElse(1000) == 10);
    }
    
    // ========== Deep ternary tests ==========
    
    function test_DeepTernary_Low() public view {
        assert(scf.deepTernary(50) == 0);
    }
    
    function test_DeepTernary_Level1() public view {
        assert(scf.deepTernary(85) == 1);
    }
    
    function test_DeepTernary_Level2() public view {
        assert(scf.deepTernary(92) == 2);
    }
    
    function test_DeepTernary_Level5() public view {
        assert(scf.deepTernary(100) == 5);
    }
    
    // ========== Multi condition tests ==========
    
    function test_MultiCondition_None() public view {
        assert(scf.multiCondition(5, 5, 5, 5) == 0);
    }
    
    function test_MultiCondition_One() public view {
        assert(scf.multiCondition(15, 5, 5, 5) == 1);
    }
    
    function test_MultiCondition_Two() public view {
        assert(scf.multiCondition(15, 15, 5, 5) == 2);
    }
    
    function test_MultiCondition_Three() public view {
        assert(scf.multiCondition(15, 15, 15, 5) == 3);
    }
    
    function test_MultiCondition_Four() public view {
        assert(scf.multiCondition(15, 15, 15, 15) == 4);
    }
    
    // ========== Complex boolean tests ==========
    
    function test_ComplexBool_AllFalse() public view {
        assert(scf.complexBool(false, false, false, false, false) == 0);
    }
    
    function test_ComplexBool_OnlyE() public view {
        assert(scf.complexBool(false, false, false, false, true) == 1);
    }
    
    function test_ComplexBool_ABandE() public view {
        assert(scf.complexBool(true, true, false, false, true) == 4);
    }
    
    function test_ComplexBool_AllTrue() public view {
        assert(scf.complexBool(true, true, true, true, true) == 4);
    }
    
    // ========== Nested for loop tests ==========
    
    function test_NestedForLoops5_Zero() public view {
        assert(scf.nestedForLoops5(0) == 0);
    }
    
    function test_NestedForLoops5_One() public view {
        assert(scf.nestedForLoops5(1) == 1);
    }
    
    function test_NestedForLoops5_Two() public view {
        // 2^5 = 32
        assert(scf.nestedForLoops5(2) == 32);
    }
    
    function test_NestedForLoops5_Three() public view {
        // 3^5 = 243
        assert(scf.nestedForLoops5(3) == 243);
    }
    
    // ========== Nested while loop tests ==========
    
    function test_NestedWhileLoops() public view {
        // n^3
        assert(scf.nestedWhileLoops(3) == 27);
        assert(scf.nestedWhileLoops(4) == 64);
    }
    
    // ========== Mixed loop tests ==========
    
    function test_MixedLoops() public view {
        // Each loop sums 0..n-1, so total = 3 * sum(0..n-1) = 3 * n*(n-1)/2
        uint256 n = 5;
        uint256 expected = 3 * (n * (n - 1) / 2);
        assert(scf.mixedLoops(n) == expected);
    }
    
    // ========== Complex break tests ==========
    
    function test_ComplexBreak() public view {
        // With target = 5, outer loop breaks at i=5 but inner loop contributes
        uint256 result = scf.complexBreak(5);
        assert(result > 0);
    }
    
    // ========== Complex continue tests ==========
    
    function test_ComplexContinue() public view {
        // Sums odd numbers not divisible by 5 and <= 90
        uint256 result = scf.complexContinue(100);
        assert(result > 0);
    }
    
    // ========== Break and continue tests ==========
    
    function test_BreakAndContinue() public view {
        // Sums numbers 1 to limit-1, skipping multiples of skip
        // limit=10, skip=3: 1+2+4+5+7+8 = 27
        assert(scf.breakAndContinue(10, 3) == 27);
    }
    
    // ========== Early return tests ==========
    
    function test_EarlyReturnInLoop() public view {
        // Target 5: inner loop at i=0 finds j=5 -> (0*10+5) = 5, returns 5 * 10 = 50
        assert(scf.earlyReturnInLoop(5) == 50);
        
        // Target 15: inner loop at i=1 finds j=5 -> (1*10+5) = 15, returns 15 * 10 = 150
        assert(scf.earlyReturnInLoop(15) == 150);
        
        // Target 0: outer loop checks i=0 first -> returns 0 * 100 = 0
        assert(scf.earlyReturnInLoop(0) == 0);
        
        // Target 1000: not found, returns 0
        assert(scf.earlyReturnInLoop(1000) == 0);
    }
    
    // ========== Nested conditionals in loop tests ==========
    
    function test_NestedConditionalsInLoop() public view {
        uint256 result = scf.nestedConditionalsInLoop(32);
        assert(result > 0);
    }
    
    // ========== Switch pattern tests ==========
    
    function test_SwitchPattern() public view {
        assert(scf.switchPattern(0) == 100);
        assert(scf.switchPattern(5) == 105);
        assert(scf.switchPattern(9) == 109);
        assert(scf.switchPattern(10) == 999);
        assert(scf.switchPattern(100) == 999);
    }
    
    // ========== Range check tests ==========
    
    function test_RangeCheck() public view {
        assert(scf.rangeCheck(5) == 1);
        assert(scf.rangeCheck(15) == 2);
        assert(scf.rangeCheck(25) == 3);
        assert(scf.rangeCheck(35) == 4);
        assert(scf.rangeCheck(45) == 5);
        assert(scf.rangeCheck(75) == 6);
        assert(scf.rangeCheck(500) == 7);
        assert(scf.rangeCheck(5000) == 8);
    }
    
    // ========== Unrolled recursion tests ==========
    
    function test_UnrolledRecursion() public view {
        assert(scf.unrolledRecursion(0) == 1);
        assert(scf.unrolledRecursion(1) == 1);
        assert(scf.unrolledRecursion(5) == 120); // 5!
        assert(scf.unrolledRecursion(10) == 3628800); // 10!
    }
    
    // ========== State machine tests ==========
    
    function test_StateMachine() public view {
        uint256 state1 = scf.stateMachine(10);
        uint256 state2 = scf.stateMachine(20);
        uint256 state3 = scf.stateMachine(50);
        
        // States should be in valid range
        assert(state1 <= 3);
        assert(state2 <= 3);
        assert(state3 <= 3);
    }
    
    // ========== Conditional accumulator tests ==========
    
    function test_ConditionalAccumulator() public view {
        // For n=15: FizzBuzz pattern
        uint256 result = scf.conditionalAccumulator(15);
        assert(result > 0);
    }
    
    // ========== Multiple exit points tests ==========
    
    function test_MultipleExitPoints_FirstExit() public view {
        assert(scf.multipleExitPoints(5, 50, 80) == 1);
    }
    
    function test_MultipleExitPoints_SecondExit() public view {
        assert(scf.multipleExitPoints(100, 10, 80) == 2);
    }
    
    function test_MultipleExitPoints_ThirdExit() public view {
        assert(scf.multipleExitPoints(100, 100, 20) == 3);
    }
    
    function test_MultipleExitPoints_NoMatch() public view {
        assert(scf.multipleExitPoints(200, 200, 200) == 0);
    }
    
    // ========== Interleaved loops tests ==========
    
    function test_InterleavedLoopsAndConditions() public view {
        uint256 result = scf.interleavedLoopsAndConditions(10);
        assert(result > 0);
    }
    
    function test_InterleavedLoopsAndConditions_Small() public view {
        uint256 result = scf.interleavedLoopsAndConditions(5);
        assert(result > 0);
    }
}
