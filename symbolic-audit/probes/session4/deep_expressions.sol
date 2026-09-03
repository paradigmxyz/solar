contract DeepExpressions {
    uint256 st;
    function poly(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) external pure returns (uint256) { unchecked { return a * b + c * d - e / (a + 1) % (b + 1) + (a ^ b) & (c | d) + ((a << 3) >> 2) * ((e & 7) + 1) - (d % (c + 1)) * (b % (a + 1)) + (a > b ? c : d) * (e < c ? 1 : 2); } }
    function polyChecked(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) external pure returns (uint256) { return a * b + c * d - e / (a + 1) % (b + 1) + ((a << 3) >> 2) * ((e & 7) + 1) - (d % (c + 1)) * (b % (a + 1)) + (a > b ? c : d) * (e < c ? 1 : 2); }
    function nestedCalls(uint256 a) external pure returns (uint256) { return _f(_f(_f(a, _f(a, a)), _f(_f(a, 1), _f(2, a))), _f(_f(_f(a, 3), 4), _f(5, _f(a, 6)))); }
    function _f(uint256 x, uint256 y) internal pure returns (uint256) { unchecked { return x * 31 + y; } }
    function manyLocals(uint256 a) external pure returns (uint256) { unchecked {
        uint256 v1 = a + 1; uint256 v2 = v1 * 2; uint256 v3 = v2 ^ a; uint256 v4 = v3 | v1; uint256 v5 = v4 & v2; uint256 v6 = v5 + v3; uint256 v7 = v6 - v4; uint256 v8 = v7 * v5;
        uint256 v9 = v8 % (v6 + 1); uint256 v10 = v9 / (v7 + 1); uint256 v11 = v10 + v8; uint256 v12 = v11 ^ v9; uint256 v13 = v12 | v10; uint256 v14 = v13 & v11; uint256 v15 = v14 + v12; uint256 v16 = v15 - v13;
        uint256 v17 = v16 * v14; uint256 v18 = v17 % (v15 + 1); uint256 v19 = v18 / (v16 + 1); uint256 v20 = v19 + v17;
        return v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10 + v11 + v12 + v13 + v14 + v15 + v16 + v17 + v18 + v19 + v20; } }
    function manyLocalsBranch(uint256 a, bool c) external pure returns (uint256) { unchecked {
        uint256 v1 = a + 1; uint256 v2 = v1 * 2; uint256 v3 = v2 ^ a; uint256 v4 = v3 | v1; uint256 v5 = v4 & v2; uint256 v6 = v5 + v3; uint256 v7 = v6 - v4; uint256 v8 = v7 * v5;
        uint256 v9 = v8 % (v6 + 1); uint256 v10 = v9 / (v7 + 1); uint256 v11 = v10 + v8; uint256 v12 = v11 ^ v9; uint256 v13 = v12 | v10; uint256 v14 = v13 & v11; uint256 v15 = v14 + v12; uint256 v16 = v15 - v13;
        if (c) { v1 = v16; v5 = v12; v9 = v3; v13 = v7; } else { v2 = v15; v6 = v11; v10 = v4; v14 = v8; }
        return v1 * 1 + v2 * 2 + v3 * 3 + v4 * 4 + v5 * 5 + v6 * 6 + v7 * 7 + v8 * 8 + v9 * 9 + v10 * 10 + v11 * 11 + v12 * 12 + v13 * 13 + v14 * 14 + v15 * 15 + v16 * 16; } }
    function manyLocalsLoop(uint256 a, uint256 n) external pure returns (uint256) { require(n < 4); unchecked {
        uint256 v1 = a + 1; uint256 v2 = v1 * 2; uint256 v3 = v2 ^ a; uint256 v4 = v3 | v1; uint256 v5 = v4 & v2; uint256 v6 = v5 + v3; uint256 v7 = v6 - v4; uint256 v8 = v7 * v5;
        uint256 v9 = v8 % (v6 + 1); uint256 v10 = v9 / (v7 + 1); uint256 v11 = v10 + v8; uint256 v12 = v11 ^ v9; uint256 v13 = v12 | v10; uint256 v14 = v13 & v11; uint256 v15 = v14 + v12; uint256 v16 = v15 - v13;
        for (uint256 i; i < n; i++) { (v1, v2, v3, v4, v5, v6, v7, v8) = (v2, v3, v4, v5, v6, v7, v8, v1 + i); (v9, v10, v11, v12, v13, v14, v15, v16) = (v16, v15, v14, v13, v12, v11, v10, v9 ^ i); }
        return v1 * 1 + v2 * 2 + v3 * 3 + v4 * 4 + v5 * 5 + v6 * 6 + v7 * 7 + v8 * 8 + v9 * 9 + v10 * 10 + v11 * 11 + v12 * 12 + v13 * 13 + v14 * 14 + v15 * 15 + v16 * 16; } }
    function manyLocalsNarrow(uint8 a, bool c) external pure returns (uint256) { unchecked {
        uint8 v1 = a + 1; int8 v2 = int8(v1) * 2; uint16 v3 = uint16(v1) ^ a; bool v4 = v3 > 5; bytes2 v5 = bytes2(v3); uint8 v6 = uint8(v3) + uint8(v2); int16 v7 = int16(v2) - int16(uint16(v1)); uint32 v8 = uint32(v3) * v6;
        uint8 v9 = v6 % (v1 + 1); int8 v10 = v2 / (int8(v6) | 1); uint16 v11 = uint16(v9) + v3; bytes1 v12 = v5[c ? 0 : 1]; bool v13 = v4 && v10 > 0; uint8 v14 = uint8(v12) & v9; uint64 v15 = uint64(v8) + v11; int8 v16 = v10 - int8(v14);
        if (c) { v1 = uint8(v15); v6 = uint8(uint16(v7)); v9 = uint8(v8); }
        return uint256(v1) + uint256(int256(v2)) + v3 + (v4 ? 1 : 0) + uint16(v5) + v6 + uint256(int256(v7)) + v8 + v9 + uint256(int256(v10)) + v11 + uint8(v12) + (v13 ? 1 : 0) + v14 + v15 + uint256(int256(v16)); } }
    function deepTernary(uint256 a) external pure returns (uint256) { return a < 1 ? 1 : a < 2 ? 2 : a < 3 ? 3 : a < 4 ? 4 : a < 5 ? 5 : a < 6 ? 6 : a < 7 ? 7 : a < 8 ? 8 : a < 9 ? 9 : a < 10 ? 10 : 11; }
    function deepBool(uint256 a, uint256 b, uint256 c) external pure returns (bool) { return ((a > b && b > c) || (a < b && b < c) || (a == b && b != c) || (a != b && b == c)) && !(a == c && b == c) || (a == 0 && b == 0 && c == 0); }
    function deepBoolSide(uint256 a) external returns (bool, uint256) { bool r = (_t(1) > a && _t(2) > a) || (_t(3) < a && _t(4) < a) || _t(5) == a; return (r, st); }
    function _t(uint256 k) internal returns (uint256) { st = st * 10 + k; return k; }
    function deepArgs(uint256 a) external pure returns (uint256) { return _ten(a, a + 1, a + 2, a + 3, a + 4, a + 5, a + 6, a + 7, a + 8, _ten(a, a, a, a, a, a, a, a, a, a)); }
    function _ten(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f, uint256 g, uint256 h, uint256 i, uint256 j) internal pure returns (uint256) { unchecked { return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i + 10 * j; } }
    function deepArgsSide(uint256 a) external returns (uint256, uint256) { uint256 r = _ten(_t(1), _t(2), _t(3), _t(4), _t(5), _t(6), _t(7), _t(8), _t(9), a); return (r, st); }
    function deepArgsMem(uint256 a) external pure returns (uint256) { uint256[] memory x = new uint256[](1); x[0] = a; return _mem(x, x, x, x, x, x, x, x, x, x); }
    function _mem(uint256[] memory a, uint256[] memory b, uint256[] memory c, uint256[] memory d, uint256[] memory e, uint256[] memory f, uint256[] memory g, uint256[] memory h, uint256[] memory i, uint256[] memory j) internal pure returns (uint256) { a[0] += 1; b[0] += 1; return a[0] + b[0] + c[0] + d[0] + e[0] + f[0] + g[0] + h[0] + i[0] + j[0]; }
    function deepReturns(uint256 a) external pure returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256) { (uint256 x1, uint256 x2, uint256 x3, uint256 x4, uint256 x5, uint256 x6) = _six(a); (uint256 y1, uint256 y2, uint256 y3, uint256 y4, uint256 y5, uint256 y6) = _six(a + 1); return (x1, x2, x3, x4, x5, x6, y1, y2, y3, y4, y5, y6); }
    function _six(uint256 a) internal pure returns (uint256, uint256, uint256, uint256, uint256, uint256) { unchecked { return (a, a * 2, a * 3, a * 4, a * 5, a * 6); } }
    function deepTupleSwap(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f, uint256 g, uint256 h) external pure returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256) { (a, b, c, d, e, f, g, h) = (h, g, f, e, d, c, b, a); (a, c, e, g) = (c, e, g, a); return (a, b, c, d, e, f, g, h); }
    function deepMemStruct(uint256 a) external pure returns (uint256) { S memory s = S(a, S2(a + 1, [a, a + 2, a + 3]), new uint256[](2)); s.arr[1] = s.inner.arr[2] + s.inner.v; return s.v + s.inner.v + s.inner.arr[0] + s.inner.arr[1] + s.inner.arr[2] + s.arr[0] + s.arr[1]; }
    struct S2 { uint256 v; uint256[3] arr; } struct S { uint256 v; S2 inner; uint256[] arr; }
    function deepIndex(uint256 a) external pure returns (uint256) { uint256[3][3][3] memory m; m[a % 3][(a + 1) % 3][(a + 2) % 3] = a; return m[a % 3][(a + 1) % 3][(a + 2) % 3] + m[0][0][0] + m[2][2][2]; }
    function deepIndexDyn(uint256 a) external pure returns (uint256) { uint256[][][] memory m = new uint256[][][](2); m[1] = new uint256[][](2); m[1][1] = new uint256[](2); m[1][1][1] = a; return m[1][1][1] + m[1][1].length + m[1].length + m.length; }
    function deepStorage(uint256 a) external returns (uint256) { st = a; st = st * 2 + st / 2 - st % 3 + (st ^ 7) + (st | 1) - (st & 2); return st; }
    function chainedCompare(uint256 a, uint256 b) external pure returns (bool) { return (a < b) == (b > a) == true != false; }
    function mixedWidthExpr(uint8 a, uint16 b, uint32 c, uint64 d) external pure returns (uint256) { return uint256(a) * b + uint256(c) * d - uint256(a) * d + (uint256(b) << 8) + (uint256(c) >> 8); }
    function mixedWidthExprChecked(uint8 a, uint16 b) external pure returns (uint16) { return uint16(a) * b + a - b; }
    function mixedSignedExpr(int8 a, int16 b, int32 c) external pure returns (int256) { return int256(a) * b + int256(c) * a - int256(b) * c + (int256(a) >> 1) + (int256(b) << 1); }
    function assignInExpr(uint256 a) external pure returns (uint256, uint256, uint256) { uint256 x; uint256 y; uint256 z = (x = a + 1) + (y = x * 2) + (x = y + 1); return (x, y, z); }
    function incInIndex(uint256 a) external pure returns (uint256, uint256) { require(a < 3); uint256[4] memory m = [uint256(10), 20, 30, 40]; uint256 i = a; uint256 r = m[i++] + m[i++]; return (r, i); }
    function incInIndexStore(uint256 a) external pure returns (uint256, uint256, uint256) { require(a < 3); uint256[4] memory m; uint256 i = a; m[i++] = i; m[++i] = i; return (m[a], m[a + 2], i); }
}
