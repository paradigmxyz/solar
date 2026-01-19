// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Arithmetic.sol";

contract ArithmeticTest {
    Arithmetic arith;

    function setUp() public {
        arith = new Arithmetic();
    }

    // ========== Basic Arithmetic ==========

    function test_AddBasic() public view {
        assert(arith.add(2, 3) == 5);
        assert(arith.add(0, 0) == 0);
        assert(arith.add(1, 0) == 1);
    }

    function test_AddLargeNumbers() public view {
        uint256 large = type(uint256).max / 2;
        assert(arith.add(large, large) == large * 2);
    }

    function test_SubBasic() public view {
        assert(arith.sub(5, 3) == 2);
        assert(arith.sub(10, 10) == 0);
        assert(arith.sub(100, 1) == 99);
    }

    function test_MulBasic() public view {
        assert(arith.mul(3, 4) == 12);
        assert(arith.mul(0, 100) == 0);
        assert(arith.mul(1, 1) == 1);
    }

    function test_MulByZero() public view {
        assert(arith.mul(12345, 0) == 0);
        assert(arith.mul(0, 12345) == 0);
    }

    function test_DivBasic() public view {
        assert(arith.div(10, 2) == 5);
        assert(arith.div(9, 3) == 3);
        assert(arith.div(0, 5) == 0);
    }

    function test_DivTruncates() public view {
        assert(arith.div(7, 2) == 3);
        assert(arith.div(10, 3) == 3);
        assert(arith.div(1, 2) == 0);
    }

    function test_ModBasic() public view {
        assert(arith.mod(10, 3) == 1);
        assert(arith.mod(9, 3) == 0);
        assert(arith.mod(7, 4) == 3);
    }

    // ========== Comparison Operators ==========

    function test_LessThan() public view {
        assert(arith.lt(1, 2) == true);
        assert(arith.lt(2, 1) == false);
        assert(arith.lt(5, 5) == false);
        assert(arith.lt(0, 1) == true);
    }

    function test_GreaterThan() public view {
        assert(arith.gt(2, 1) == true);
        assert(arith.gt(1, 2) == false);
        assert(arith.gt(5, 5) == false);
    }

    function test_LessOrEqual() public view {
        assert(arith.lte(1, 2) == true);
        assert(arith.lte(5, 5) == true);
        assert(arith.lte(6, 5) == false);
    }

    function test_GreaterOrEqual() public view {
        assert(arith.gte(2, 1) == true);
        assert(arith.gte(5, 5) == true);
        assert(arith.gte(4, 5) == false);
    }

    function test_Equality() public view {
        assert(arith.eq(5, 5) == true);
        assert(arith.eq(0, 0) == true);
        assert(arith.eq(5, 6) == false);
    }

    function test_NotEqual() public view {
        assert(arith.neq(5, 6) == true);
        assert(arith.neq(5, 5) == false);
    }

    // ========== Bitwise Operations ==========

    function test_BitwiseAnd() public view {
        assert(arith.bitwiseAnd(0xF0, 0x0F) == 0x00);
        assert(arith.bitwiseAnd(0xFF, 0x0F) == 0x0F);
        assert(arith.bitwiseAnd(0xAB, 0xAB) == 0xAB);
    }

    function test_BitwiseOr() public view {
        assert(arith.bitwiseOr(0xF0, 0x0F) == 0xFF);
        assert(arith.bitwiseOr(0x00, 0x00) == 0x00);
    }

    function test_BitwiseXor() public view {
        assert(arith.bitwiseXor(0xFF, 0xFF) == 0x00);
        assert(arith.bitwiseXor(0xAA, 0x55) == 0xFF);
    }

    // ========== Signed Arithmetic ==========

    function test_SignedAdd() public view {
        assert(arith.signedAdd(5, 3) == 8);
        assert(arith.signedAdd(-5, 3) == -2);
        assert(arith.signedAdd(-5, -3) == -8);
        assert(arith.signedAdd(0, 0) == 0);
    }

    function test_SignedSub() public view {
        assert(arith.signedSub(5, 3) == 2);
        assert(arith.signedSub(3, 5) == -2);
        assert(arith.signedSub(-5, -3) == -2);
        assert(arith.signedSub(-5, 3) == -8);
    }

    function test_SignedMul() public view {
        assert(arith.signedMul(3, 4) == 12);
        assert(arith.signedMul(-3, 4) == -12);
        assert(arith.signedMul(-3, -4) == 12);
        assert(arith.signedMul(0, -100) == 0);
    }

    function test_SignedDiv() public view {
        assert(arith.signedDiv(10, 2) == 5);
        assert(arith.signedDiv(-10, 2) == -5);
        assert(arith.signedDiv(10, -2) == -5);
        assert(arith.signedDiv(-10, -2) == 5);
        assert(arith.signedDiv(-7, 2) == -3);
    }

    function test_SignedLt() public view {
        assert(arith.signedLt(-5, 0) == true);
        assert(arith.signedLt(-10, -5) == true);
        assert(arith.signedLt(5, -5) == false);
        assert(arith.signedLt(0, 0) == false);
    }

    function test_SignedGt() public view {
        assert(arith.signedGt(0, -5) == true);
        assert(arith.signedGt(-5, -10) == true);
        assert(arith.signedGt(-5, 5) == false);
        assert(arith.signedGt(0, 0) == false);
    }

    function test_ShiftLeft() public view {
        assert(arith.shiftLeft(1, 0) == 1);
        assert(arith.shiftLeft(1, 1) == 2);
        assert(arith.shiftLeft(1, 8) == 256);
        assert(arith.shiftLeft(0xFF, 8) == 0xFF00);
    }

    function test_ShiftRight() public view {
        assert(arith.shiftRight(256, 8) == 1);
        assert(arith.shiftRight(255, 4) == 15);
        assert(arith.shiftRight(1, 1) == 0);
    }

    // ========== Complex Expressions ==========

    function test_ComplexExpr() public view {
        assert(arith.complexExpr(10, 5, 2) == 29);
    }

    // ========== Increment/Decrement ==========

    function test_PreIncrement() public {
        arith.resetCounter();
        assert(arith.preIncrement() == 1);
        assert(arith.counter() == 1);
    }

    function test_PostIncrement() public {
        arith.resetCounter();
        assert(arith.postIncrement() == 0);
        assert(arith.counter() == 1);
    }

    function test_PreDecrement() public {
        arith.setCounter(5);
        assert(arith.preDecrement() == 4);
        assert(arith.counter() == 4);
    }

    function test_PostDecrement() public {
        arith.setCounter(5);
        assert(arith.postDecrement() == 5);
        assert(arith.counter() == 4);
    }

    // ========== Compound Assignments ==========

    function test_AddAssign() public {
        arith.resetValue();
        arith.addAssign(10);
        assert(arith.value() == 10);
        arith.addAssign(5);
        assert(arith.value() == 15);
    }

    function test_SubAssign() public {
        arith.setValue(100);
        arith.subAssign(30);
        assert(arith.value() == 70);
    }

    function test_MulAssign() public {
        arith.setValue(5);
        arith.mulAssign(4);
        assert(arith.value() == 20);
    }

    function test_DivAssign() public {
        arith.setValue(100);
        arith.divAssign(5);
        assert(arith.value() == 20);
    }
}
