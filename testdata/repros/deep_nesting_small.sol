// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract DeepNesting {
    function nested(uint256 x) public pure returns (uint256) {
        uint256 result = x;
        if (result > 0) {
            result = result + 1;
            if (result > 1) {
                result = result + 1;
                if (result > 2) {
                    result = result + 1;
                    if (result > 3) {
                        result = result + 1;
                        if (result > 4) {
                            result = result + 1;
                            if (result > 5) {
                                result = result + 1;
                                if (result > 6) {
                                    result = result + 1;
                                    if (result > 7) {
                                        result = result + 1;
                                        if (result > 8) {
                                            result = result + 1;
                                            if (result > 9) {
                                                result = result + 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        return result;
    }
    function nestedLoops(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i0 = 0; i0 < n; i0++) {
            for (uint256 i1 = 0; i1 < n; i1++) {
                for (uint256 i2 = 0; i2 < n; i2++) {
                    for (uint256 i3 = 0; i3 < n; i3++) {
                        for (uint256 i4 = 0; i4 < n; i4++) {
                            for (uint256 i5 = 0; i5 < n; i5++) {
                                for (uint256 i6 = 0; i6 < n; i6++) {
                                    for (uint256 i7 = 0; i7 < n; i7++) {
                                        for (uint256 i8 = 0; i8 < n; i8++) {
                                            for (uint256 i9 = 0; i9 < n; i9++) {
                                                sum += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        return sum;
    }
}
