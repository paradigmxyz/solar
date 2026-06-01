// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Local variable as second operand in binary ops
/// @notice Tests for the stack underflow bug when local variable is used as second operand (b) 
/// in binary ops and is used again later.
contract LocalSecondOperand {
    uint256 public x;

    /// @notice Tests local variable used as second operand in division, then reused
    function divideByLocal() external returns (uint256 a, uint256 b) {
        uint256 _x = x;
        a = 1000 / _x;  // _x is second operand
        b = 2000 / _x;  // _x is used again
    }

    /// @notice Tests local variable used as first operand (should work)
    function localAsFirst() external returns (uint256 a, uint256 b) {
        uint256 _x = x;
        a = _x + 1;  // _x is first operand
        b = _x + 2;  // _x is used again
    }

    /// @notice Tests local variable as second operand in subtraction
    function subtractFromLocal() external returns (uint256 a, uint256 b) {
        uint256 _x = x;
        a = 1000 - _x;  // _x is second operand
        b = 500 - _x;   // _x is used again
    }

    /// @notice Tests local variable as second operand in modulo
    function moduloByLocal() external returns (uint256 a, uint256 b) {
        uint256 _x = x;
        a = 1000 % _x;  // _x is second operand
        b = 2000 % _x;  // _x is used again
    }

    /// @notice Helper to set x for testing
    function setX(uint256 val) external {
        x = val;
    }
}
