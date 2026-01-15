// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Solar vs Solc Showcase
/// @notice Test cases that trigger "Stack Too Deep" in Solc

contract Showcase {
    
    // ============ TEST 1: 16 params + many locals ============
    // The ultimate stress test - guaranteed stack overflow in solc
    
    function ultimateStressTest(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h,
        uint256 i, uint256 j, uint256 k, uint256 l,
        uint256 m, uint256 n, uint256 o, uint256 p
    ) public pure returns (uint256) {
        uint256 sum1 = a + b + c + d;
        uint256 sum2 = e + f + g + h;
        uint256 sum3 = i + j + k + l;
        uint256 sum4 = m + n + o + p;
        
        uint256 prod1 = a * b;
        uint256 prod2 = c * d;
        uint256 prod3 = e * f;
        uint256 prod4 = g * h;
        
        uint256 diff1 = (i > j) ? i - j : j - i;
        uint256 diff2 = (k > l) ? k - l : l - k;
        uint256 diff3 = (m > n) ? m - n : n - m;
        uint256 diff4 = (o > p) ? o - p : p - o;
        
        uint256 combo1 = sum1 + prod1 + diff1;
        uint256 combo2 = sum2 + prod2 + diff2;
        uint256 combo3 = sum3 + prod3 + diff3;
        uint256 combo4 = sum4 + prod4 + diff4;
        
        uint256 final1 = combo1 * combo2;
        uint256 final2 = combo3 * combo4;
        
        return final1 + final2 + a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p;
    }
    
    // ============ TEST 2: Multi-step AMM calculation ============
    // Real DeFi pattern: swap calculation with validation
    
    function calculateSwap(
        uint256 reserve0,
        uint256 reserve1,
        uint256 fee,
        uint256 amountIn,
        uint256 slippageTolerance,
        uint256 minOut,
        uint256 maxOut,
        uint256 priceLimit
    ) public pure returns (uint256 amountOut, uint256 priceImpact, uint256 effectiveFee) {
        uint256 amountInWithFee = amountIn * (10000 - fee);
        uint256 numerator = amountInWithFee * reserve1;
        uint256 denominator = (reserve0 * 10000) + amountInWithFee;
        amountOut = numerator / denominator;
        
        uint256 spotPrice = (reserve1 * 1e18) / reserve0;
        uint256 executionPrice = (amountOut * 1e18) / amountIn;
        priceImpact = spotPrice > executionPrice 
            ? ((spotPrice - executionPrice) * 10000) / spotPrice
            : 0;
            
        effectiveFee = amountIn - (amountOut * reserve0 / reserve1);
        
        // Use all params to prevent optimization
        require(amountOut >= minOut, "insufficient output");
        require(amountOut <= maxOut, "excessive output");
        require(priceImpact <= slippageTolerance, "slippage");
        require(executionPrice <= priceLimit, "price limit");
        
        return (amountOut, priceImpact, effectiveFee);
    }
    
    // ============ TEST 3: Batch calculation ============
    // 10 amounts + computed intermediate values
    
    function batchSum(
        uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5,
        uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10
    ) public pure returns (uint256) {
        uint256 sum1 = a1 + a2;
        uint256 sum2 = a3 + a4;
        uint256 sum3 = a5 + a6;
        uint256 sum4 = a7 + a8;
        uint256 sum5 = a9 + a10;
        
        uint256 prod1 = a1 * a2;
        uint256 prod2 = a3 * a4;
        uint256 prod3 = a5 * a6;
        uint256 prod4 = a7 * a8;
        uint256 prod5 = a9 * a10;
        
        uint256 total = sum1 + sum2 + sum3 + sum4 + sum5;
        uint256 prodTotal = prod1 + prod2 + prod3 + prod4 + prod5;
        
        // Use all intermediate values to prevent optimization
        return total + prodTotal + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10;
    }
    
    // ============ TEST 4: Chained arithmetic ============
    // Deep dependency chain with all variables live
    
    function chainedArithmetic(
        uint256 x1, uint256 x2, uint256 x3, uint256 x4,
        uint256 x5, uint256 x6, uint256 x7, uint256 x8
    ) public pure returns (uint256) {
        uint256 y1 = x1 + x2;
        uint256 y2 = x3 + x4;
        uint256 y3 = x5 + x6;
        uint256 y4 = x7 + x8;
        
        uint256 z1 = y1 * y2;
        uint256 z2 = y3 * y4;
        
        uint256 w1 = z1 + z2;
        uint256 w2 = z1 * z2;
        
        uint256 v1 = w1 + x1 + x2 + x3 + x4;
        uint256 v2 = w2 + x5 + x6 + x7 + x8;
        
        uint256 u1 = v1 * v2;
        uint256 u2 = v1 + v2;
        
        // Reference all intermediate values
        return u1 + u2 + y1 + y2 + y3 + y4 + z1 + z2 + w1 + w2;
    }
    
    // ============ TEST 5: Simple baseline (should work everywhere) ============
    
    function simpleAdd(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
}
