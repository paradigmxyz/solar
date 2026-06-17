// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Arithmetic edge cases
/// @notice Tests for arithmetic operations including edge cases
contract Arithmetic {
    // Basic operations
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function sub(uint256 a, uint256 b) public pure returns (uint256) {
        return a - b;
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }

    function div(uint256 a, uint256 b) public pure returns (uint256) {
        return a / b;
    }

    function mod(uint256 a, uint256 b) public pure returns (uint256) {
        return a % b;
    }

    // Signed operations
    function signedAdd(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    function signedSub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    function signedMul(int256 a, int256 b) public pure returns (int256) {
        return a * b;
    }

    function signedDiv(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    // Comparison operators
    function lt(uint256 a, uint256 b) public pure returns (bool) {
        return a < b;
    }

    function gt(uint256 a, uint256 b) public pure returns (bool) {
        return a > b;
    }

    function lte(uint256 a, uint256 b) public pure returns (bool) {
        return a <= b;
    }

    function gte(uint256 a, uint256 b) public pure returns (bool) {
        return a >= b;
    }

    function eq(uint256 a, uint256 b) public pure returns (bool) {
        return a == b;
    }

    function neq(uint256 a, uint256 b) public pure returns (bool) {
        return a != b;
    }

    // Signed comparisons
    function signedLt(int256 a, int256 b) public pure returns (bool) {
        return a < b;
    }

    function signedGt(int256 a, int256 b) public pure returns (bool) {
        return a > b;
    }

    // Bitwise operations
    function bitwiseAnd(uint256 a, uint256 b) public pure returns (uint256) {
        return a & b;
    }

    function bitwiseOr(uint256 a, uint256 b) public pure returns (uint256) {
        return a | b;
    }

    function bitwiseXor(uint256 a, uint256 b) public pure returns (uint256) {
        return a ^ b;
    }

    function bitwiseNot(uint256 a) public pure returns (uint256) {
        return ~a;
    }

    function shiftLeft(uint256 a, uint256 bits) public pure returns (uint256) {
        return a << bits;
    }

    function shiftRight(uint256 a, uint256 bits) public pure returns (uint256) {
        return a >> bits;
    }

    // Complex expressions
    function complexExpr(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        return (a + b) * c - (a / (b + 1));
    }

    // Increment/decrement
    uint256 public counter;

    function preIncrement() public returns (uint256) {
        return ++counter;
    }

    function postIncrement() public returns (uint256) {
        return counter++;
    }

    function preDecrement() public returns (uint256) {
        return --counter;
    }

    function postDecrement() public returns (uint256) {
        return counter--;
    }

    function resetCounter() public {
        counter = 0;
    }

    function setCounter(uint256 val) public {
        counter = val;
    }

    // Compound assignments
    uint256 public value;

    function addAssign(uint256 x) public {
        value += x;
    }

    function subAssign(uint256 x) public {
        value -= x;
    }

    function mulAssign(uint256 x) public {
        value *= x;
    }

    function divAssign(uint256 x) public {
        value /= x;
    }

    function resetValue() public {
        value = 0;
    }

    function setValue(uint256 x) public {
        value = x;
    }
}
