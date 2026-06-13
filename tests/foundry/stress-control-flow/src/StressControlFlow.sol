// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for deep nesting, loops, and branches
/// @notice Tests compiler handling of complex control flow patterns

contract StressControlFlow {
    // ========== Deep if-else nesting (10 levels) ==========
    
    function deepIfElse(uint256 x) public pure returns (uint256) {
        if (x > 900) {
            if (x > 950) {
                if (x > 975) {
                    if (x > 990) {
                        if (x > 995) {
                            if (x > 998) {
                                if (x > 999) {
                                    return 10;
                                } else {
                                    return 9;
                                }
                            } else {
                                return 8;
                            }
                        } else {
                            return 7;
                        }
                    } else {
                        return 6;
                    }
                } else {
                    return 5;
                }
            } else {
                return 4;
            }
        } else if (x > 500) {
            return 3;
        } else if (x > 100) {
            return 2;
        } else {
            return 1;
        }
    }
    
    // ========== Deep nested ternary ==========
    
    function deepTernary(uint256 x) public pure returns (uint256) {
        return x > 80 
            ? (x > 90 
                ? (x > 95 
                    ? (x > 98 
                        ? (x > 99 ? 5 : 4) 
                        : 3) 
                    : 2) 
                : 1) 
            : 0;
    }
    
    // ========== Multiple condition chain ==========
    
    function multiCondition(uint256 a, uint256 b, uint256 c, uint256 d) public pure returns (uint256) {
        if (a > 10 && b > 10 && c > 10 && d > 10) {
            return 4;
        } else if (a > 10 && b > 10 && c > 10) {
            return 3;
        } else if (a > 10 && b > 10) {
            return 2;
        } else if (a > 10) {
            return 1;
        } else {
            return 0;
        }
    }
    
    // ========== Complex boolean logic ==========
    
    function complexBool(bool a, bool b, bool c, bool d, bool e) public pure returns (uint256) {
        if ((a && b) || (c && d)) {
            if (e) {
                return 4;
            } else if (a && b && c && d) {
                return 3;
            } else {
                return 2;
            }
        } else if (a || b || c || d || e) {
            return 1;
        } else {
            return 0;
        }
    }
    
    // ========== Nested for loops (5 levels) ==========
    
    function nestedForLoops5(uint256 n) public pure returns (uint256) {
        uint256 count = 0;
        for (uint256 a = 0; a < n; a++) {
            for (uint256 b = 0; b < n; b++) {
                for (uint256 c = 0; c < n; c++) {
                    for (uint256 d = 0; d < n; d++) {
                        for (uint256 e = 0; e < n; e++) {
                            count++;
                        }
                    }
                }
            }
        }
        return count;
    }
    
    // ========== Nested while loops ==========
    
    function nestedWhileLoops(uint256 n) public pure returns (uint256) {
        uint256 count = 0;
        uint256 i = 0;
        while (i < n) {
            uint256 j = 0;
            while (j < n) {
                uint256 k = 0;
                while (k < n) {
                    count++;
                    k++;
                }
                j++;
            }
            i++;
        }
        return count;
    }
    
    // ========== Mixed loop types ==========
    
    function mixedLoops(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        
        // For loop
        for (uint256 i = 0; i < n; i++) {
            sum += i;
        }
        
        // While loop
        uint256 j = 0;
        while (j < n) {
            sum += j;
            j++;
        }
        
        // Do-while style using while(true) with break
        uint256 k = 0;
        while (true) {
            if (k >= n) break;
            sum += k;
            k++;
        }
        
        return sum;
    }
    
    // ========== Complex break patterns ==========
    
    function complexBreak(uint256 target) public pure returns (uint256) {
        uint256 count = 0;
        
        for (uint256 i = 0; i < 100; i++) {
            if (i == target) {
                break;
            }
            
            for (uint256 j = 0; j < 100; j++) {
                if (j == target) {
                    break;
                }
                count++;
            }
            
            if (count > target * 10) {
                break;
            }
        }
        
        return count;
    }
    
    // ========== Complex continue patterns ==========
    
    function complexContinue(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        
        for (uint256 i = 0; i < n; i++) {
            // Skip even numbers
            if (i % 2 == 0) continue;
            
            // Skip multiples of 5
            if (i % 5 == 0) continue;
            
            // Skip numbers greater than 90
            if (i > 90) continue;
            
            sum += i;
        }
        
        return sum;
    }
    
    // ========== Combined break and continue ==========
    
    function breakAndContinue(uint256 limit, uint256 skip) public pure returns (uint256) {
        uint256 sum = 0;
        
        for (uint256 i = 0; i < 1000; i++) {
            if (i >= limit) {
                break;
            }
            
            if (i % skip == 0) {
                continue;
            }
            
            sum += i;
        }
        
        return sum;
    }
    
    // ========== Early return in loops ==========
    
    function earlyReturnInLoop(uint256 target) public pure returns (uint256) {
        for (uint256 i = 0; i < 100; i++) {
            if (i == target) {
                return i * 100;
            }
            
            for (uint256 j = 0; j < 10; j++) {
                if (i * 10 + j == target) {
                    return (i * 10 + j) * 10;
                }
            }
        }
        return 0;
    }
    
    // ========== Deeply nested conditionals in loops ==========
    
    function nestedConditionalsInLoop(uint256 n) public pure returns (uint256) {
        uint256 result = 0;
        
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 0) {
                if (i % 4 == 0) {
                    if (i % 8 == 0) {
                        if (i % 16 == 0) {
                            result += 16;
                        } else {
                            result += 8;
                        }
                    } else {
                        result += 4;
                    }
                } else {
                    result += 2;
                }
            } else {
                result += 1;
            }
        }
        
        return result;
    }
    
    // ========== Switch-like pattern using if-else ==========
    
    function switchPattern(uint256 x) public pure returns (uint256) {
        if (x == 0) {
            return 100;
        } else if (x == 1) {
            return 101;
        } else if (x == 2) {
            return 102;
        } else if (x == 3) {
            return 103;
        } else if (x == 4) {
            return 104;
        } else if (x == 5) {
            return 105;
        } else if (x == 6) {
            return 106;
        } else if (x == 7) {
            return 107;
        } else if (x == 8) {
            return 108;
        } else if (x == 9) {
            return 109;
        } else {
            return 999;
        }
    }
    
    // ========== Range checks ==========
    
    function rangeCheck(uint256 x) public pure returns (uint256) {
        if (x >= 0 && x < 10) {
            return 1;
        } else if (x >= 10 && x < 20) {
            return 2;
        } else if (x >= 20 && x < 30) {
            return 3;
        } else if (x >= 30 && x < 40) {
            return 4;
        } else if (x >= 40 && x < 50) {
            return 5;
        } else if (x >= 50 && x < 100) {
            return 6;
        } else if (x >= 100 && x < 1000) {
            return 7;
        } else {
            return 8;
        }
    }
    
    // ========== Recursive-like patterns (unrolled) ==========
    
    function unrolledRecursion(uint256 n) public pure returns (uint256) {
        uint256 result = 1;
        
        if (n >= 1) result *= 1;
        if (n >= 2) result *= 2;
        if (n >= 3) result *= 3;
        if (n >= 4) result *= 4;
        if (n >= 5) result *= 5;
        if (n >= 6) result *= 6;
        if (n >= 7) result *= 7;
        if (n >= 8) result *= 8;
        if (n >= 9) result *= 9;
        if (n >= 10) result *= 10;
        
        return result;
    }
    
    // ========== State machine pattern ==========
    
    function stateMachine(uint256 input) public pure returns (uint256) {
        uint256 state = 0;
        
        for (uint256 i = 0; i < input; i++) {
            if (state == 0) {
                if (i % 3 == 0) {
                    state = 1;
                }
            } else if (state == 1) {
                if (i % 5 == 0) {
                    state = 2;
                } else if (i % 7 == 0) {
                    state = 0;
                }
            } else if (state == 2) {
                if (i % 11 == 0) {
                    state = 3;
                } else if (i % 2 == 0) {
                    state = 1;
                }
            } else if (state == 3) {
                if (i % 13 == 0) {
                    state = 0;
                }
            }
        }
        
        return state;
    }
    
    // ========== Accumulator with multiple conditions ==========
    
    function conditionalAccumulator(uint256 n) public pure returns (uint256) {
        uint256 acc = 0;
        
        for (uint256 i = 1; i <= n; i++) {
            if (i % 15 == 0) {
                acc += i * 4;
            } else if (i % 5 == 0) {
                acc += i * 3;
            } else if (i % 3 == 0) {
                acc += i * 2;
            } else {
                acc += i;
            }
        }
        
        return acc;
    }
    
    // ========== Loop with multiple exit points ==========
    
    function multipleExitPoints(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        for (uint256 i = 0; i < 100; i++) {
            if (i == a) {
                return 1;
            }
            if (i == b) {
                return 2;
            }
            if (i == c) {
                return 3;
            }
            
            if (i > 50) {
                for (uint256 j = 0; j < 10; j++) {
                    if (i + j == a + b) {
                        return 4;
                    }
                }
            }
        }
        return 0;
    }
    
    // ========== Interleaved loops and conditions ==========
    
    function interleavedLoopsAndConditions(uint256 n) public pure returns (uint256) {
        uint256 result = 0;
        
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 0) {
                for (uint256 j = 0; j < i; j++) {
                    if (j % 2 == 0) {
                        result += j;
                    } else {
                        result += j * 2;
                    }
                }
            } else {
                uint256 k = i;
                while (k > 0) {
                    if (k % 3 == 0) {
                        result += k;
                    }
                    k--;
                }
            }
        }
        
        return result;
    }
}
