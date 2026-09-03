contract ControlFlow {
    uint256 st;
    function doWhileContinue(uint256 n) external pure returns (uint256 s) {
        require(n < 10);
        uint256 i;
        do { i++; if (i % 2 == 0) continue; s += i; } while (i < n);
    }
    function doWhileBreak(uint256 n) external pure returns (uint256 s) {
        uint256 i;
        do { s += i; if (i == n) break; i++; } while (i < 5);
    }
    function doWhileOnce(bool c) external pure returns (uint256 s) { do { s++; } while (c && s < 3); }
    function nestedBreak(uint256 a, uint256 b) external pure returns (uint256 s) {
        require(a < 6 && b < 6);
        for (uint256 i; i < a; i++) {
            for (uint256 j; j < b; j++) { if (j == 2) break; if (i == 3) continue; s += i * 10 + j; }
            if (i == 4) break;
        }
    }
    function returnInLoop(uint256[] calldata a, uint256 t) external pure returns (uint256 idx) {
        for (idx = 0; idx < a.length; idx++) if (a[idx] == t) return idx;
        return type(uint256).max;
    }
    function namedReturnBare(uint256 x) external pure returns (uint256 r) { r = x + 1; if (x > 5) return r; r = 0; }
    function namedReturnUnset(uint256 x) external pure returns (uint256 r, bool b) { if (x > 5) { r = 1; return (r, b); } b = true; }
    function returnTupleNamed(uint256 x) external pure returns (uint256 r, uint256 q) { r = x; return (q, r); }
    function whileSideEffect(uint256 n) external pure returns (uint256 s, uint256 i) { require(n < 8); while (i++ < n) s += i; }
    function forEmptyParts(uint256 n) external pure returns (uint256 i) { require(n < 8); for (;;) { if (i >= n) break; i++; } }
    function forNoInit(uint256 n) external pure returns (uint256 s) { require(n < 8); uint256 i = 1; for (; i <= n; ++i) s += i; }
    function forCondSide(uint256 n) external pure returns (uint256 s) { require(n < 8); uint256 i; for (; i++ < n;) s += i; }
    function forContinueInc(uint256 n) external pure returns (uint256 s) { require(n < 8); for (uint256 i; i < n; i++) { if (i == 1) continue; s += i; } }
    function forDeclShadow(uint256 n) external pure returns (uint256 s) { require(n < 5); for (uint256 i; i < n; i++) { uint256 t = i * 2; s += t; } uint256 t = 100; s += t; }
    function ternaryNested(uint256 x) external pure returns (uint256) { return x < 3 ? (x == 0 ? 10 : 20) : (x > 7 ? 30 : 40); }
    function ifChain(uint256 x) external pure returns (uint256) { if (x == 0) return 0; else if (x < 10) return 1; else if (x < 100) return 2; return 3; }
    function blockScopes(uint256 x) external pure returns (uint256 r) { { uint256 y = x + 1; r = y; } { uint256 y = x + 2; r += y; } }
    function unreachableAfterReturn(uint256 x) external pure returns (uint256) { if (true) return x; }
    function loopWithRevert(uint256 n) external pure returns (uint256 s) { for (uint256 i; i < n; i++) { s += i; require(s < 10, "big"); } }
    function loopWithAssert(uint256 n) external pure returns (uint256 s) { for (uint256 i; i < n; i++) { s += i; assert(s < 10); } }
    function whileFalse() external pure returns (uint256 s) { while (false) s++; return s + 1; }
    function ifFalseConst(uint256 x) external pure returns (uint256) { if (x == x + 0 && false) return 1; return 2; }
    function complexCond(uint256 a, uint256 b, bool c) external pure returns (uint256) { if ((a > b && !c) || (a == b && c) || a == 0) return 1; return 0; }
    function shortCircuitSide(uint256 a) external pure returns (uint256 x) { bool r = a > 0 && ++x > 0 || ++x > 1; return r ? x : x + 100; }
    function nestedTernaryTypes(bool c, uint8 a, uint16 b) external pure returns (uint256) { return c ? a : b; }
    function continueDoWhileCond(uint256 n) external pure returns (uint256 cnt) { require(n < 6); uint256 i; do { cnt++; if (cnt > 20) break; continue; } while (++i < n); }
    function modifierState(uint256 v) external returns (uint256) { st = v; return _mod(v); }
    function _mod(uint256 v) internal returns (uint256) { st += v; return st; }
    modifier twice() { _; _; }
    modifier none() { if (false) _; }
    modifier before(uint256 v) { st += v; _; }
    modifier after_() { _; st = st * 2; }
    modifier earlyReturn(bool c) { if (c) return; _; }
    function withTwice(uint256 v) external twice returns (uint256) { st += v; return st; }
    function withNone(uint256 v) external none returns (uint256 r) { r = v; }
    function withBefore(uint256 v) external before(v * 2) returns (uint256) { return st; }
    function withAfter(uint256 v) external after_ returns (uint256) { st = v; return st; }
    function withAfterRead(uint256 v) external after_ returns (uint256) { st = v; return _read(); }
    function _read() internal view returns (uint256) { return st; }
    function withEarly(bool c, uint256 v) external earlyReturn(c) returns (uint256 r) { r = v; }
    function withEarlyMulti(bool c, uint256 v) external earlyReturn(c) twice returns (uint256 r) { r += v; }
    function chained(uint256 v) external before(1) after_ twice returns (uint256) { st += v; return st; }
    function retInMod(uint256 v) external before(v) returns (uint256) { if (v > 3) return 99; return st; }
    function tryNoExt(uint256 v) external pure returns (uint256) { uint256 r = v; { r++; } return r; }
    function gotoLike(uint256 n) external pure returns (uint256 r) { require(n < 20); uint256 i; while (true) { if (i >= n) break; i += 3; r++; } }
    function deepNest(uint256 a) external pure returns (uint256 r) { for (uint256 i; i < 2; i++) for (uint256 j; j < 2; j++) for (uint256 k; k < 2; k++) if (a == i + j + k) r++; }
    function loopStorage(uint256 n) external returns (uint256) { require(n < 8); for (uint256 i; i < n; i++) st += i; return st; }
    function loopStorageBreak(uint256 n) external returns (uint256) { for (uint256 i; ; i++) { if (i == n || i == 5) break; st++; } return st; }
    function uncheckedLoop(uint8 n) external pure returns (uint8 i) { unchecked { for (i = 250; i < 255 && i != n; i++) {} } }
    function uncheckedLoopWrap() external pure returns (uint256 cnt) { unchecked { for (uint8 i = 250; i != 2; i++) cnt++; } }
    function checkedLoopOverflow(uint8 start) external pure returns (uint256 cnt) { for (uint8 i = start; i < 255; i++) cnt++; if (cnt > 0) { uint8 i = 255; i++; } }
}
