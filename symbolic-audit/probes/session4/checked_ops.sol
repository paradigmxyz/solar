contract CheckedOps {
    uint8 su8; int8 si8; uint256 su; uint8[] arr; mapping(uint256 => uint8) m; mapping(uint256 => int16) mi;
    struct S { uint8 a; int8 b; uint256 c; }
    S s; uint256 tick;
    function _idx() internal returns (uint256) { tick++; return 0; }
    function addAssign(uint8 a, uint8 b) external pure returns (uint8) { a += b; return a; }
    function subAssign(uint8 a, uint8 b) external pure returns (uint8) { a -= b; return a; }
    function mulAssign(uint8 a, uint8 b) external pure returns (uint8) { a *= b; return a; }
    function divAssign(uint8 a, uint8 b) external pure returns (uint8) { a /= b; return a; }
    function modAssign(uint8 a, uint8 b) external pure returns (uint8) { a %= b; return a; }
    function shlAssign(uint8 a, uint8 b) external pure returns (uint8) { a <<= b; return a; }
    function shrAssign(uint8 a, uint8 b) external pure returns (uint8) { a >>= b; return a; }
    function andAssign(uint8 a, uint8 b) external pure returns (uint8) { a &= b; return a; }
    function orAssign(uint8 a, uint8 b) external pure returns (uint8) { a |= b; return a; }
    function xorAssign(uint8 a, uint8 b) external pure returns (uint8) { a ^= b; return a; }
    function addAssignI(int8 a, int8 b) external pure returns (int8) { a += b; return a; }
    function subAssignI(int8 a, int8 b) external pure returns (int8) { a -= b; return a; }
    function mulAssignI(int8 a, int8 b) external pure returns (int8) { a *= b; return a; }
    function divAssignI(int8 a, int8 b) external pure returns (int8) { a /= b; return a; }
    function shlAssignI(int8 a, uint8 b) external pure returns (int8) { a <<= b; return a; }
    function shrAssignI(int8 a, uint8 b) external pure returns (int8) { a >>= b; return a; }
    function addAssignU(uint8 a, uint8 b) external pure returns (uint8) { unchecked { a += b; } return a; }
    function mulAssignU(uint8 a, uint8 b) external pure returns (uint8) { unchecked { a *= b; } return a; }
    function shlAssignU(uint8 a, uint8 b) external pure returns (uint8) { unchecked { a <<= b; } return a; }
    function assignValue(uint8 a, uint8 b) external pure returns (uint8, uint8) { uint8 r = (a += b); return (r, a); }
    function assignChain(uint8 a, uint8 b) external pure returns (uint8, uint8, uint8) { uint8 c; uint8 d; c = d = a + b; return (c, d, a); }
    function assignInCond(uint8 a) external pure returns (uint8) { uint8 b; if ((b = a) > 3) return b; return 0; }
    function preInc(uint8 a) external pure returns (uint8, uint8) { uint8 r = ++a; return (r, a); }
    function postInc(uint8 a) external pure returns (uint8, uint8) { uint8 r = a++; return (r, a); }
    function preDec(uint8 a) external pure returns (uint8, uint8) { uint8 r = --a; return (r, a); }
    function postDec(uint8 a) external pure returns (uint8, uint8) { uint8 r = a--; return (r, a); }
    function preIncI(int8 a) external pure returns (int8, int8) { int8 r = ++a; return (r, a); }
    function postDecI(int8 a) external pure returns (int8, int8) { int8 r = a--; return (r, a); }
    function incU(uint8 a) external pure returns (uint8, uint8) { unchecked { uint8 r = a++; return (r, a); } }
    function decU(int8 a) external pure returns (int8, int8) { unchecked { int8 r = --a; return (r, a); } }
    function incInExpr(uint8 a) external pure returns (uint256) { return uint256(a++) + uint256(++a); }
    function incTwice(uint8 a) external pure returns (uint8) { a++; a++; return a; }
    function storageInc(uint8 v) external returns (uint8, uint8) { su8 = v; uint8 r = su8++; return (r, su8); }
    function storagePreInc(uint8 v) external returns (uint8, uint8) { su8 = v; uint8 r = ++su8; return (r, su8); }
    function storageDecI(int8 v) external returns (int8, int8) { si8 = v; int8 r = si8--; return (r, si8); }
    function storageCompound(uint8 v, uint8 d) external returns (uint8) { su8 = v; su8 *= d; return su8; }
    function storageCompoundI(int8 v, int8 d) external returns (int8) { si8 = v; si8 -= d; return si8; }
    function storageCompoundShl(uint8 v, uint8 d) external returns (uint8) { su8 = v; su8 <<= d; return su8; }
    function storageCompoundWide(uint256 v, uint256 d) external returns (uint256) { su = v; su += d; su -= 1; return su; }
    function arrInc(uint8 v) external returns (uint8, uint8, uint256) { arr.push(v); uint8 r = arr[_idx()]++; return (r, arr[0], tick); }
    function arrCompound(uint8 v, uint8 d) external returns (uint8, uint256) { arr.push(v); arr[_idx()] += d; return (arr[0], tick); }
    function arrCompoundSide(uint8 v, uint8 d) external returns (uint8, uint256) { arr.push(v); arr.push(v); arr[_idx()] += _bump(d); return (arr[0], tick); }
    function _bump(uint8 d) internal returns (uint8) { tick += 10; return d; }
    function mapInc(uint8 v) external returns (uint8, uint8) { m[1] = v; uint8 r = m[1]++; return (r, m[1]); }
    function mapCompound(uint8 v, uint8 d) external returns (uint8, uint256) { m[0] = v; m[_idx()] -= d; return (m[0], tick); }
    function mapCompoundI(int16 v, int16 d) external returns (int16) { mi[3] = v; mi[3] *= d; return mi[3]; }
    function structInc(uint8 v) external returns (uint8, uint8) { s.a = v; uint8 r = s.a++; return (r, s.a); }
    function structCompound(int8 v, int8 d) external returns (int8) { s.b = v; s.b += d; return s.b; }
    function structCompoundWide(uint256 v) external returns (uint256) { s.c = v; s.c <<= 1; s.c |= 1; return s.c; }
    function memStructInc(uint8 v) external pure returns (uint8, uint8) { S memory ms = S(v, 0, 0); uint8 r = ms.a++; return (r, ms.a); }
    function memArrInc(uint8 v) external pure returns (uint8, uint8) { uint8[] memory a = new uint8[](1); a[0] = v; uint8 r = ++a[0]; return (r, a[0]); }
    function memArrCompound(uint8 v, uint8 d) external pure returns (uint8) { uint8[2] memory a; a[1] = v; a[1] *= d; return a[1]; }
    function tupleCompound(uint8 a, uint8 b) external pure returns (uint8, uint8) { (a, b) = (b, a); a += b; return (a, b); }
    function divCompoundZero(uint8 a) external pure returns (uint8) { uint8 z = 0; a /= z; return a; }
    function modCompoundZero(int8 a) external pure returns (int8) { int8 z = 0; a %= z; return a; }
    function divMinCompound(int8 a) external pure returns (int8) { int8 n = -1; a /= n; return a; }
    function shlWide(uint256 a, uint256 b) external pure returns (uint256) { a <<= b; return a; }
    function shrWide(uint256 a, uint256 b) external pure returns (uint256) { a >>= b; return a; }
    function shlWideLit(uint256 a) external pure returns (uint256) { a <<= 255; return a; }
    function shrIWideLit(int256 a) external pure returns (int256) { a >>= 255; return a; }
    function checkedThenUnchecked(uint8 a, uint8 b) external pure returns (uint8, uint8) { uint8 c = a + b; unchecked { c += a; } return (c, a + b); }
    function uncheckedNested(uint8 a, uint8 b) external pure returns (uint8) { unchecked { uint8 c = a + b; { c *= b; } return c; } }
    function uncheckedCallChecked(uint8 a, uint8 b) external pure returns (uint8) { unchecked { return _checkedAdd(a, b); } }
    function _checkedAdd(uint8 a, uint8 b) internal pure returns (uint8) { return a + b; }
    function checkedCallUnchecked(uint8 a, uint8 b) external pure returns (uint8) { return _uncheckedAdd(a, b) + 1; }
    function _uncheckedAdd(uint8 a, uint8 b) internal pure returns (uint8) { unchecked { return a + b; } }
    function uncheckedDiv(uint8 a) external pure returns (uint8) { unchecked { uint8 z = 0; return a / z; } }
    function uncheckedShl(uint8 a, uint256 s) external pure returns (uint8) { unchecked { return a << s; } }
    function uncheckedNeg(uint8 a) external pure returns (uint8) { unchecked { return uint8(-int8(a)); } }
    function uncheckedIdx(uint8 i) external pure returns (uint8) { unchecked { uint8[3] memory a = [uint8(1), 2, 3]; return a[i]; } }
    function uncheckedEnum(uint8 i) external pure returns (E) { unchecked { return E(i); } }
    enum E { X, Y }
    function uncheckedExp(uint8 a, uint8 b) external pure returns (uint8) { unchecked { return a ** b; } }
    function uncheckedExpWide(uint256 a, uint256 b) external pure returns (uint256) { unchecked { return a ** b; } }
    function uncheckedSub(uint256 a, uint256 b) external pure returns (uint256) { unchecked { return a - b; } }
    function uncheckedMul(uint256 a, uint256 b) external pure returns (uint256) { unchecked { return a * b; } }
    function uncheckedAddNarrow(uint16 a, uint16 b) external pure returns (uint16, uint256) { unchecked { uint16 c = a + b; return (c, c); } }
    function uncheckedIncLoop(uint8 n) external pure returns (uint8 i) { unchecked { for (i = 0; i < n; ++i) {} } }
    function uncheckedSubLoop(uint8 n) external pure returns (uint8 i) { unchecked { for (i = n; i > 0; --i) {} return i - 1; } }
    function uncheckedShlNarrowWiden(uint8 a) external pure returns (uint256) { unchecked { return uint256(a << 4); } }
    function uncheckedMulWiden(uint8 a, uint8 b) external pure returns (uint256) { unchecked { return uint256(a * b); } }
    function uncheckedNegWiden(int8 a) external pure returns (int256) { unchecked { return int256(-a); } }
    function uncheckedIncWiden(uint8 a) external pure returns (uint256) { unchecked { a++; return a; } }
    function uncheckedAddCmp(uint8 a, uint8 b) external pure returns (bool) { unchecked { return a + b < a; } }
    function uncheckedMulCmp(int8 a, int8 b) external pure returns (bool) { unchecked { return a * b == -128; } }
    function uncheckedIdxWrap(uint8 i) external pure returns (uint8) { unchecked { uint8[] memory a = new uint8[](3); a[0] = 7; return a[i + 255 + 1]; } }
    function uncheckedShiftIdx(uint8 i) external pure returns (uint8) { unchecked { uint8[] memory a = new uint8[](4); a[3] = 9; return a[i >> 6]; } }
}
