// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test: Commutative operations can swap operands to minimize DUP depth
// ADD, MUL, AND, OR, XOR, EQ are commutative
contract CommutativeReorder {
    uint256 public a;
    uint256 public b;
    
    // a + b == b + a, so we can choose the order that minimizes stack ops
    function add() external view returns (uint256) {
        uint256 _a = a;
        uint256 _b = b;
        // _a is used once more after this, _b is not
        // Ideally we'd emit _b first (goes deeper), then _a on top
        // After ADD, _a's position is better for next use
        return _a + _b + _a;
    }
    
    // Multiple commutative operations
    function multipleAdd() external view returns (uint256) {
        uint256 _a = a;
        uint256 _b = b;
        return (_a + _b) * (_b + _a);
    }
    
    // Mix of commutative and non-commutative
    function mixedOps() external view returns (uint256) {
        uint256 _a = a;
        uint256 _b = b;
        // SUB is not commutative, MUL is
        return (_a - _b) * (_a + _b);
    }
}
