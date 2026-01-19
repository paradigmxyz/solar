// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Control flow edge cases
/// @notice Tests for conditionals, loops, break/continue
contract ControlFlow {

    // ========== Conditionals ==========

    function simpleIf(uint256 x) public pure returns (uint256) {
        if (x > 10) {
            return 1;
        }
        return 0;
    }

    function ifElse(uint256 x) public pure returns (uint256) {
        if (x > 10) {
            return 2;
        } else {
            return 1;
        }
    }

    function ifElseIf(uint256 x) public pure returns (uint256) {
        if (x > 100) {
            return 3;
        } else if (x > 10) {
            return 2;
        } else {
            return 1;
        }
    }

    function nestedIf(uint256 x, uint256 y) public pure returns (uint256) {
        if (x > 10) {
            if (y > 10) {
                return 4;
            } else {
                return 3;
            }
        } else {
            if (y > 10) {
                return 2;
            } else {
                return 1;
            }
        }
    }

    // Ternary operator
    function ternary(uint256 x) public pure returns (uint256) {
        return x > 10 ? 100 : 50;
    }

    function nestedTernary(uint256 x) public pure returns (uint256) {
        return x > 100 ? 3 : (x > 10 ? 2 : 1);
    }

    // ========== For Loops ==========

    function forLoopSum(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < n; i++) {
            sum = sum + i;
        }
        return sum;
    }

    function forLoopProduct(uint256 n) public pure returns (uint256) {
        if (n == 0) return 0;
        uint256 product = 1;
        for (uint256 i = 1; i <= n; i++) {
            product = product * i;
        }
        return product;
    }

    function nestedForLoop(uint256 rows, uint256 cols) public pure returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < rows; i++) {
            for (uint256 j = 0; j < cols; j++) {
                count = count + 1;
            }
        }
        return count;
    }

    // ========== While Loops ==========

    function whileLoopSum(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        uint256 i = 0;
        while (i < n) {
            sum = sum + i;
            i = i + 1;
        }
        return sum;
    }

    function whileLoopCountdown(uint256 n) public pure returns (uint256) {
        uint256 count = 0;
        while (n > 0) {
            count = count + 1;
            n = n - 1;
        }
        return count;
    }

    // ========== Break and Continue ==========

    function forWithBreak(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < 100; i++) {
            if (i >= n) {
                break;
            }
            sum = sum + i;
        }
        return sum;
    }

    function forWithContinue(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 0) {
                continue;
            }
            sum = sum + i; // Only add odd numbers
        }
        return sum;
    }

    function whileWithBreak(uint256 target) public pure returns (uint256) {
        uint256 i = 0;
        while (true) {
            if (i >= target) {
                break;
            }
            i = i + 1;
        }
        return i;
    }

    function nestedLoopBreak(uint256 limit) public pure returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < 10; i++) {
            for (uint256 j = 0; j < 10; j++) {
                if (count >= limit) {
                    break; // Only breaks inner loop
                }
                count = count + 1;
            }
            if (count >= limit) {
                break; // Breaks outer loop
            }
        }
        return count;
    }

    // ========== Edge Cases ==========

    function zeroIterations() public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < 0; i++) {
            sum = sum + 1;
        }
        return sum; // Should be 0
    }

    function singleIteration() public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < 1; i++) {
            sum = sum + 10;
        }
        return sum; // Should be 10
    }

    function earlyReturn(uint256 x) public pure returns (uint256) {
        if (x == 0) {
            return 999;
        }
        for (uint256 i = 0; i < 10; i++) {
            if (i == x) {
                return i * 100;
            }
        }
        return 0;
    }
}
