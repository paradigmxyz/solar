// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test: Same operand used multiple times benefits from keeping it on stack
// Before optimization: Multiple DUPs to fetch the same value
// After optimization: Keep value near top of stack, minimize DUP depth
contract RepeatedOperand {
    uint256 public x;
    uint256 public y;
    
    // x is used 3 times - should benefit from stack scheduling
    function multipleUse(uint256 a) external view returns (uint256) {
        return x + x * a + x / 2;
    }
    
    // Both x and y used multiple times
    function bothMultiple() external view returns (uint256) {
        return x * y + x * y + x - y;
    }
}
