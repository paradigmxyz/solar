// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Test: Deep expression trees that stress stack scheduling
// Deep DUPs (DUP9+) indicate poor scheduling
contract DeepExpression {
    uint256 public a;
    uint256 public b;
    uint256 public c;
    uint256 public d;
    
    // Deep nesting should keep intermediate values accessible
    function deepNest() external view returns (uint256) {
        return a + (b + (c + (d + (a * b) + (c * d))));
    }
    
    // Many operands - tests stack pressure
    function manyOperands() external view returns (uint256) {
        return a + b + c + d + a + b + c + d;
    }
    
    // Polynomial with repeated base
    function polynomial(uint256 x) external pure returns (uint256) {
        // x used 4 times - should stay near stack top
        return x * x * x * x + 3 * x * x + 2 * x + 1;
    }
    
    // Nested multiplications
    function nestedMul() external view returns (uint256) {
        return a * b * c * d * a * b;
    }
}
