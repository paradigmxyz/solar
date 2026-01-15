// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StackDeep {
    // This function uses many local variables to trigger "Stack Too Deep" in solc
    function manyLocals(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h
    ) public pure returns (uint256) {
        uint256 v1 = a + b;
        uint256 v2 = c + d;
        uint256 v3 = e + f;
        uint256 v4 = g + h;
        uint256 v5 = v1 + v2;
        uint256 v6 = v3 + v4;
        uint256 v7 = v5 + v6;
        uint256 v8 = v7 + a;
        uint256 v9 = v8 + b;
        uint256 v10 = v9 + c;
        uint256 v11 = v10 + d;
        uint256 v12 = v11 + e;
        // Use all variables to prevent optimization
        return v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + f + g + h;
    }

    // Even more extreme case - 20+ active variables
    function extremeLocals(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h,
        uint256 i, uint256 j, uint256 k, uint256 l
    ) public pure returns (uint256) {
        uint256 r1 = a * b;
        uint256 r2 = c * d;
        uint256 r3 = e * f;
        uint256 r4 = g * h;
        uint256 r5 = i * j;
        uint256 r6 = k * l;
        uint256 r7 = r1 + r2;
        uint256 r8 = r3 + r4;
        uint256 r9 = r5 + r6;
        uint256 r10 = r7 + r8;
        uint256 r11 = r9 + r10;
        uint256 r12 = r11 + a + b + c + d;
        uint256 r13 = r12 + e + f + g + h;
        uint256 r14 = r13 + i + j + k + l;
        // Reference everything
        return r1 + r2 + r3 + r4 + r5 + r6 + r7 + r8 + r9 + r10 + r11 + r12 + r13 + r14;
    }
}
