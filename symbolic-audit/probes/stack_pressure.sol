contract StackPressure {
    uint256 s0; uint256 s1; mapping(uint256 => uint256) m;
    function h(uint256 x, uint256 y) internal pure returns (uint256, uint256) { unchecked { return (x * 3 + y, x ^ (y << 1)); } }
    function k(uint256 x) internal pure returns (uint256) { unchecked { return x * 0x9e3779b97f4a7c15 + 1; } }
    function f0(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 & v1);
            uint256 v4 = (v0 | v3);
            uint256 v5 = (v0 | v2);
            uint256 v6 = (v0 - v4);
            uint256 v7 = (v0 ^ v6);
            uint256 v8 = (v6 - v0);
            uint256 v9 = (v1 + v6);
            uint256 v10 = (v9 - v1);
            uint256 v11 = (v10 + v9);
            uint256 v12 = (v9 ^ v11);
            if (v0 & 1 == 1) { v3 = (v0 - v8); } else { v4 = k(v6); }
            for (uint256 i = 0; i < (v2 & 3); i++) { v8 = (v1 * v9) + i; (v8, v10) = h(v2, v1); }
            m[v9 & 7] = v9; s0 = v10; v3 = m[v5 & 7] + s0;
            if (v1 == 0) revert(); if (v8 > v11) { v1 = (v9 | v0); return (v3 & v7); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13);
        }
    }
    function f1(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 ^ v2);
            uint256 v4 = (v3 * v1);
            uint256 v5 = (v1 & v4);
            uint256 v6 = (v1 | v0);
            uint256 v7 = (v2 ^ v4);
            uint256 v8 = (v5 ^ v7);
            uint256 v9 = (v4 + v1);
            uint256 v10 = (v8 - v6);
            uint256 v11 = (v5 ^ v2);
            uint256 v12 = (v6 & v0);
            uint256 v13 = (v1 | v8);
            uint256 v14 = (v12 * v5);
            uint256 v15 = (v11 | v5);
            if (v15 & 1 == 1) { v14 = (v2 + v13); } else { v8 = k(v15); }
            for (uint256 i = 0; i < (v2 & 3); i++) { v1 = (v9 | v10) + i; (v14, v9) = h(v12, v11); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16);
        }
    }
    function f2(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 - v2);
            uint256 v4 = (v0 + v1);
            uint256 v5 = (v1 - v2);
            uint256 v6 = (v5 ^ v1);
            uint256 v7 = (v3 + v6);
            if (v2 & 1 == 1) { v7 = (v6 * v4); } else { v2 = k(v6); }
            for (uint256 i = 0; i < (v4 & 3); i++) { v6 = (v5 ^ v7) + i; (v3, v2) = h(v1, v2); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }
    function f3(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 + v2);
            uint256 v4 = (v3 - v2);
            uint256 v5 = (v2 + v4);
            uint256 v6 = (v1 | v3);
            uint256 v7 = (v2 | v4);
            uint256 v8 = (v5 & v1);
            uint256 v9 = (v8 ^ v0);
            if (v8 & 1 == 1) { v6 = (v6 ^ v9); } else { v1 = k(v7); }
            for (uint256 i = 0; i < (v6 & 3); i++) { v0 = (v3 - v1) + i; (v7, v2) = h(v1, v5); }
            m[v9 & 7] = v0; s0 = v1; v0 = m[v9 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10);
        }
    }
    function f4(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 * v0);
            uint256 v4 = (v0 - v3);
            uint256 v5 = (v4 - v3);
            uint256 v6 = (v5 * v2);
            uint256 v7 = (v4 ^ v2);
            uint256 v8 = (v1 ^ v0);
            uint256 v9 = (v7 ^ v8);
            if (v4 & 1 == 1) { v1 = (v2 & v1); } else { v5 = k(v4); }
            for (uint256 i = 0; i < (v7 & 3); i++) { v2 = (v8 - v0) + i; (v8, v5) = h(v2, v8); }
            if (v0 == 0) revert(); if (v8 > v4) { v1 = (v4 * v8); return (v2 - v5); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10);
        }
    }
    function f5(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 & v1);
            uint256 v4 = (v1 - v2);
            uint256 v5 = (v1 & v3);
            uint256 v6 = (v1 | v5);
            uint256 v7 = (v3 & v2);
            uint256 v8 = (v0 * v7);
            uint256 v9 = (v7 - v4);
            uint256 v10 = (v9 ^ v5);
            uint256 v11 = (v5 + v10);
            uint256 v12 = (v3 - v1);
            uint256 v13 = (v7 * v3);
            uint256 v14 = (v3 | v7);
            uint256 v15 = (v14 + v9);
            if (v15 & 1 == 1) { v11 = (v2 & v13); } else { v3 = k(v12); }
            for (uint256 i = 0; i < (v6 & 3); i++) { v15 = (v5 & v6) + i; (v10, v2) = h(v12, v14); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16);
        }
    }
    function f6(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 & v0);
            uint256 v4 = (v1 - v0);
            uint256 v5 = (v0 | v1);
            uint256 v6 = (v3 | v1);
            uint256 v7 = (v6 ^ v4);
            uint256 v8 = (v5 | v1);
            uint256 v9 = (v8 + v2);
            uint256 v10 = (v0 | v1);
            uint256 v11 = (v2 - v6);
            uint256 v12 = (v3 * v0);
            uint256 v13 = (v3 | v4);
            if (v3 & 1 == 1) { v12 = (v9 * v5); } else { v8 = k(v6); }
            for (uint256 i = 0; i < (v13 & 3); i++) { v2 = (v0 * v11) + i; (v7, v10) = h(v9, v13); }
            m[v8 & 7] = v6; s0 = v13; v8 = m[v2 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14);
        }
    }
    function f7(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 ^ v2);
            uint256 v4 = (v1 + v2);
            uint256 v5 = (v1 - v4);
            uint256 v6 = (v3 & v4);
            uint256 v7 = (v0 + v4);
            uint256 v8 = (v5 | v7);
            uint256 v9 = (v8 + v7);
            uint256 v10 = (v8 - v0);
            uint256 v11 = (v3 + v4);
            uint256 v12 = (v1 ^ v8);
            uint256 v13 = (v8 + v0);
            uint256 v14 = (v7 | v5);
            uint256 v15 = (v8 | v9);
            if (v6 & 1 == 1) { v8 = (v14 | v8); } else { v15 = k(v7); }
            for (uint256 i = 0; i < (v8 & 3); i++) { v6 = (v14 ^ v2) + i; (v3, v12) = h(v14, v10); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16);
        }
    }
    function f8(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 ^ v0);
            uint256 v4 = (v0 & v3);
            uint256 v5 = (v2 - v0);
            uint256 v6 = (v5 - v2);
            uint256 v7 = (v2 ^ v1);
            uint256 v8 = (v3 + v5);
            if (v6 & 1 == 1) { v7 = (v2 - v3); } else { v6 = k(v8); }
            for (uint256 i = 0; i < (v6 & 3); i++) { v5 = (v6 * v3) + i; (v5, v1) = h(v5, v0); }
            if (v5 == 0) revert(); if (v8 > v7) { v7 = (v0 * v6); return (v8 | v4); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9);
        }
    }
    function f9(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 + v2);
            uint256 v4 = (v0 * v1);
            uint256 v5 = (v0 * v1);
            uint256 v6 = (v1 & v3);
            uint256 v7 = (v6 ^ v2);
            uint256 v8 = (v2 | v4);
            if (v7 & 1 == 1) { v5 = (v1 + v4); } else { v2 = k(v6); }
            for (uint256 i = 0; i < (v1 & 3); i++) { v4 = (v0 * v1) + i; (v1, v3) = h(v1, v4); }
            m[v1 & 7] = v7; s0 = v0; v5 = m[v8 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9);
        }
    }
    function f10(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 + v0);
            uint256 v4 = (v1 - v0);
            uint256 v5 = (v2 - v0);
            uint256 v6 = (v1 & v2);
            uint256 v7 = (v2 - v4);
            uint256 v8 = (v4 | v3);
            uint256 v9 = (v2 * v4);
            uint256 v10 = (v0 + v4);
            uint256 v11 = (v0 & v10);
            uint256 v12 = (v8 - v11);
            uint256 v13 = (v8 - v7);
            if (v7 & 1 == 1) { v1 = (v10 ^ v13); } else { v10 = k(v7); }
            for (uint256 i = 0; i < (v8 & 3); i++) { v13 = (v6 * v8) + i; (v11, v3) = h(v3, v5); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14);
        }
    }
    function f11(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 ^ v0);
            uint256 v4 = (v2 - v0);
            uint256 v5 = (v0 & v4);
            uint256 v6 = (v5 ^ v2);
            uint256 v7 = (v1 + v0);
            uint256 v8 = (v6 | v7);
            uint256 v9 = (v4 & v3);
            uint256 v10 = (v4 ^ v0);
            if (v2 & 1 == 1) { v2 = (v4 + v7); } else { v4 = k(v5); }
            for (uint256 i = 0; i < (v5 & 3); i++) { v8 = (v5 + v3) + i; (v4, v3) = h(v5, v2); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11);
        }
    }
    function f12(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 + v2);
            uint256 v4 = (v3 | v1);
            uint256 v5 = (v1 | v4);
            uint256 v6 = (v0 * v5);
            uint256 v7 = (v6 - v0);
            if (v6 & 1 == 1) { v0 = (v6 * v0); } else { v4 = k(v3); }
            for (uint256 i = 0; i < (v1 & 3); i++) { v2 = (v6 * v7) + i; (v7, v2) = h(v4, v2); }
            m[v0 & 7] = v6; s0 = v2; v0 = m[v3 & 7] + s0;
            if (v1 == 0) revert(); if (v0 > v0) { v2 = (v5 ^ v0); return (v7 + v4); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }
    function f13(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 ^ v2);
            uint256 v4 = (v2 ^ v0);
            uint256 v5 = (v0 & v4);
            uint256 v6 = (v4 & v0);
            uint256 v7 = (v5 * v3);
            uint256 v8 = (v1 * v6);
            uint256 v9 = (v3 - v8);
            uint256 v10 = (v7 ^ v9);
            uint256 v11 = (v1 & v7);
            uint256 v12 = (v4 | v0);
            uint256 v13 = (v10 - v12);
            uint256 v14 = (v1 - v9);
            uint256 v15 = (v5 & v4);
            uint256 v16 = (v9 | v15);
            uint256 v17 = (v4 ^ v0);
            if (v1 & 1 == 1) { v15 = (v8 & v3); } else { v6 = k(v15); }
            for (uint256 i = 0; i < (v9 & 3); i++) { v16 = (v9 ^ v14) + i; (v14, v3) = h(v17, v6); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16) ^ (v16 * 17) ^ (v17 * 18);
        }
    }
    function f14(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 + v1);
            uint256 v4 = (v2 + v1);
            uint256 v5 = (v4 * v3);
            uint256 v6 = (v3 - v1);
            uint256 v7 = (v0 + v4);
            uint256 v8 = (v2 | v5);
            uint256 v9 = (v4 - v5);
            uint256 v10 = (v9 * v8);
            uint256 v11 = (v1 - v5);
            if (v7 & 1 == 1) { v7 = (v6 - v0); } else { v0 = k(v7); }
            for (uint256 i = 0; i < (v10 & 3); i++) { v7 = (v6 & v4) + i; (v2, v6) = h(v5, v6); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12);
        }
    }
    function f15(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 + v1);
            uint256 v4 = (v2 ^ v1);
            uint256 v5 = (v0 & v1);
            uint256 v6 = (v0 * v2);
            uint256 v7 = (v2 ^ v0);
            uint256 v8 = (v6 | v7);
            uint256 v9 = (v1 ^ v5);
            uint256 v10 = (v4 * v0);
            uint256 v11 = (v1 & v0);
            uint256 v12 = (v4 - v10);
            if (v3 & 1 == 1) { v4 = (v6 * v8); } else { v3 = k(v12); }
            for (uint256 i = 0; i < (v5 & 3); i++) { v12 = (v6 & v0) + i; (v6, v8) = h(v8, v3); }
            m[v11 & 7] = v1; s0 = v0; v11 = m[v6 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13);
        }
    }
    function f16(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 & v0);
            uint256 v4 = (v2 + v1);
            uint256 v5 = (v4 - v1);
            uint256 v6 = (v3 * v5);
            uint256 v7 = (v2 * v6);
            uint256 v8 = (v4 & v3);
            uint256 v9 = (v3 ^ v4);
            uint256 v10 = (v8 + v6);
            uint256 v11 = (v2 + v10);
            uint256 v12 = (v3 ^ v8);
            uint256 v13 = (v8 ^ v3);
            uint256 v14 = (v5 ^ v12);
            if (v6 & 1 == 1) { v2 = (v8 - v3); } else { v1 = k(v2); }
            for (uint256 i = 0; i < (v5 & 3); i++) { v8 = (v1 - v5) + i; (v5, v4) = h(v12, v9); }
            if (v3 == 0) revert(); if (v14 > v0) { v11 = (v13 ^ v6); return (v6 | v11); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15);
        }
    }
    function f17(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 * v2);
            uint256 v4 = (v0 * v1);
            uint256 v5 = (v4 - v2);
            uint256 v6 = (v5 | v4);
            uint256 v7 = (v5 + v1);
            uint256 v8 = (v4 ^ v1);
            uint256 v9 = (v6 ^ v7);
            uint256 v10 = (v4 - v0);
            if (v0 & 1 == 1) { v6 = (v7 ^ v9); } else { v0 = k(v1); }
            for (uint256 i = 0; i < (v6 & 3); i++) { v8 = (v7 - v10) + i; (v1, v3) = h(v2, v2); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11);
        }
    }
    function f18(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 & v0);
            uint256 v4 = (v3 | v0);
            uint256 v5 = (v0 - v4);
            uint256 v6 = (v1 + v4);
            uint256 v7 = (v5 * v6);
            uint256 v8 = (v2 * v5);
            uint256 v9 = (v8 & v6);
            uint256 v10 = (v1 + v9);
            uint256 v11 = (v4 | v8);
            uint256 v12 = (v3 * v6);
            uint256 v13 = (v3 + v9);
            uint256 v14 = (v0 * v8);
            uint256 v15 = (v7 * v4);
            if (v7 & 1 == 1) { v15 = (v7 - v8); } else { v0 = k(v13); }
            for (uint256 i = 0; i < (v9 & 3); i++) { v1 = (v0 ^ v3) + i; (v13, v2) = h(v8, v7); }
            m[v13 & 7] = v11; s0 = v7; v15 = m[v1 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16);
        }
    }
    function f19(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 * v2);
            uint256 v4 = (v3 + v0);
            uint256 v5 = (v2 - v0);
            uint256 v6 = (v3 * v1);
            uint256 v7 = (v6 - v1);
            uint256 v8 = (v7 * v1);
            uint256 v9 = (v4 | v1);
            uint256 v10 = (v7 - v2);
            uint256 v11 = (v7 & v6);
            uint256 v12 = (v0 - v9);
            uint256 v13 = (v6 - v0);
            uint256 v14 = (v0 - v9);
            uint256 v15 = (v6 & v0);
            uint256 v16 = (v1 ^ v2);
            uint256 v17 = (v14 & v10);
            uint256 v18 = (v3 - v2);
            if (v10 & 1 == 1) { v6 = (v5 & v16); } else { v14 = k(v1); }
            for (uint256 i = 0; i < (v9 & 3); i++) { v12 = (v11 ^ v10) + i; (v5, v3) = h(v0, v2); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16) ^ (v16 * 17) ^ (v17 * 18) ^ (v18 * 19);
        }
    }
    function f20(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 ^ v1);
            uint256 v4 = (v0 - v2);
            uint256 v5 = (v3 * v2);
            uint256 v6 = (v3 + v0);
            uint256 v7 = (v5 - v3);
            uint256 v8 = (v5 ^ v4);
            uint256 v9 = (v3 * v5);
            uint256 v10 = (v7 & v0);
            uint256 v11 = (v6 & v3);
            if (v6 & 1 == 1) { v0 = (v6 ^ v0); } else { v1 = k(v0); }
            for (uint256 i = 0; i < (v4 & 3); i++) { v3 = (v11 | v1) + i; (v5, v5) = h(v4, v5); }
            if (v9 == 0) revert(); if (v0 > v4) { v11 = (v11 * v5); return (v4 & v0); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12);
        }
    }
    function f21(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v2 + v0);
            uint256 v4 = (v1 ^ v0);
            uint256 v5 = (v3 * v4);
            uint256 v6 = (v3 - v5);
            uint256 v7 = (v3 + v1);
            uint256 v8 = (v4 & v6);
            uint256 v9 = (v2 * v3);
            uint256 v10 = (v5 * v7);
            uint256 v11 = (v9 | v1);
            uint256 v12 = (v3 - v6);
            uint256 v13 = (v3 + v6);
            uint256 v14 = (v10 ^ v0);
            uint256 v15 = (v8 * v14);
            uint256 v16 = (v5 + v6);
            uint256 v17 = (v2 | v8);
            uint256 v18 = (v2 + v6);
            uint256 v19 = (v13 & v15);
            if (v14 & 1 == 1) { v5 = (v7 ^ v4); } else { v14 = k(v19); }
            for (uint256 i = 0; i < (v7 & 3); i++) { v17 = (v3 * v9) + i; (v8, v18) = h(v8, v11); }
            m[v8 & 7] = v8; s0 = v6; v14 = m[v7 & 7] + s0;
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16) ^ (v16 * 17) ^ (v17 * 18) ^ (v18 * 19) ^ (v19 * 20);
        }
    }
    function f22(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v0 - v2);
            uint256 v4 = (v2 - v3);
            uint256 v5 = (v2 ^ v0);
            uint256 v6 = (v2 | v1);
            uint256 v7 = (v4 & v1);
            uint256 v8 = (v1 ^ v5);
            uint256 v9 = (v0 + v1);
            if (v7 & 1 == 1) { v3 = (v7 + v5); } else { v4 = k(v3); }
            for (uint256 i = 0; i < (v1 & 3); i++) { v0 = (v3 + v9) + i; (v5, v8) = h(v2, v7); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10);
        }
    }
    function f23(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = (v1 + v0);
            uint256 v4 = (v2 + v0);
            uint256 v5 = (v2 - v4);
            uint256 v6 = (v0 * v1);
            uint256 v7 = (v0 & v4);
            uint256 v8 = (v3 + v6);
            uint256 v9 = (v5 & v6);
            uint256 v10 = (v5 | v2);
            uint256 v11 = (v4 - v1);
            uint256 v12 = (v0 | v7);
            uint256 v13 = (v7 ^ v1);
            uint256 v14 = (v1 ^ v12);
            uint256 v15 = (v10 - v8);
            uint256 v16 = (v2 - v10);
            if (v12 & 1 == 1) { v8 = (v13 & v9); } else { v9 = k(v13); }
            for (uint256 i = 0; i < (v1 & 3); i++) { v9 = (v11 ^ v13) + i; (v0, v11) = h(v6, v12); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16) ^ (v16 * 17);
        }
    }
}
