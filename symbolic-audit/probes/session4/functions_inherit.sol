contract A { uint256 public x; function f() public virtual returns (uint256) { x += 1; return 1; } function g(uint256 v) public virtual returns (uint256) { return v; } function h() internal virtual returns (uint256) { return 10; } }
contract B is A { function f() public virtual override returns (uint256) { return super.f() + 10; } function h() internal virtual override returns (uint256) { return super.h() + 20; } }
contract C is A { function f() public virtual override returns (uint256) { return super.f() + 100; } function h() internal virtual override returns (uint256) { return super.h() + 200; } }
contract D is B, C {
    function f() public override(B, C) returns (uint256) { return super.f() + 1000; }
    function h() internal override(B, C) returns (uint256) { return super.h() + 2000; }
    function callH() external returns (uint256) { return h(); }
    function callF() external returns (uint256, uint256) { uint256 r = f(); return (r, x); }
    function callG() external returns (uint256) { return g(5); }
    function callAf() external returns (uint256) { return A.f(); }
    function callBf() external returns (uint256) { return B.f(); }
    function ov(uint256 v) public pure returns (uint256) { return v; }
    function ov(int256 v) public pure returns (uint256) { return 2000 + uint256(v); }
    function ov(bytes memory) public pure returns (uint256) { return 3000; }
    function ov(uint256 a, uint256 b) public pure returns (uint256) { return a * 10 + b; }
    function ovCall(uint256 v) external pure returns (uint256, uint256, uint256, uint256) { return (ov(v), ov(v + 1), ov(int256(v)), ov(v, v)); }
    function ovLit() external pure returns (uint256, uint256, uint256) { return (ov(uint256(3)), ov(int256(3)), ov("")); }
    function recur(uint256 n) public pure returns (uint256) { if (n == 0) return 0; return n + recur(n - 1); }
    function recurCall(uint256 n) external pure returns (uint256) { require(n < 10); return recur(n); }
    function fib(uint256 n) internal pure returns (uint256) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
    function fibCall(uint256 n) external pure returns (uint256) { require(n < 8); return fib(n); }
    function mutual(uint256 n) external pure returns (uint256) { require(n < 10); return _even(n) ? 1 : 0; }
    function _even(uint256 n) internal pure returns (bool) { return n == 0 ? true : _odd(n - 1); }
    function _odd(uint256 n) internal pure returns (bool) { return n == 0 ? false : _even(n - 1); }
    function manyRet(uint256 v) external pure returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256) { return (v, v + 1, v + 2, v + 3, v + 4, v + 5, v + 6, v + 7, v + 8, v + 9); }
    function manyArgs(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f_, uint256 g_, uint256 h_, uint256 i, uint256 j, uint256 k, uint256 l) external pure returns (uint256) { return _many(a, b, c, d, e, f_, g_, h_, i, j, k, l); }
    function _many(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f_, uint256 g_, uint256 h_, uint256 i, uint256 j, uint256 k, uint256 l) internal pure returns (uint256) { return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f_ + 7 * g_ + 8 * h_ + 9 * i + 10 * j + 11 * k + 12 * l; }
    function manyRetInternal(uint256 v) external pure returns (uint256) { (uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f_, uint256 g_, uint256 h_) = _eight(v); return a + b * 2 + c * 3 + d * 4 + e * 5 + f_ * 6 + g_ * 7 + h_ * 8; }
    function _eight(uint256 v) internal pure returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256) { return (v, v + 1, v + 2, v + 3, v + 4, v + 5, v + 6, v + 7); }
    function skipRet(uint256 v) external pure returns (uint256, uint256) { (, uint256 b, , uint256 d, , , , ) = _eight(v); (uint256 a, , , , , , , uint256 hh) = _eight(v + 1); return (a + b, d + hh); }
    function unnamedRet(uint256 v) external pure returns (uint256, bool) { return _un(v); }
    function _un(uint256 v) internal pure returns (uint256, bool) { if (v > 3) return (v, true); return (0, false); }
    function mixedNamed(uint256 v) external pure returns (uint256 a, uint256) { a = v; return (a, a + 1); }
    function freeFn(uint256 v) external pure returns (uint256) { return free(v) + FREE_CONST; }
    function constUse() external pure returns (uint256, bytes32, string memory, address) { return (K, KB, KS, KA); }
    uint256 constant K = 2 ** 200 + 7; bytes32 constant KB = keccak256("k"); string constant KS = "a constant string that is long enough"; address constant KA = address(0x1234);
    function constExpr() external pure returns (uint256, uint256) { return (K / 3 + 1, uint256(KB) % 7); }
    function tupleSwapLocals(uint256 a, uint256 b) external pure returns (uint256, uint256) { (a, b) = (b, a); (a, b) = (a + b, a - b); return (a, b); }
    function tupleFromCall(uint256 v) external pure returns (uint256) { (uint256 a, bool b) = _un(v); return b ? a : 7; }
    function nestedCall(uint256 v) external pure returns (uint256) { return recur(recur(v % 3)); }
    function argEval(uint256 v) external returns (uint256) { return _two(_inc(), _inc()) + v; }
    uint256 cnt;
    function _inc() internal returns (uint256) { return ++cnt; }
    function _two(uint256 a, uint256 b) internal pure returns (uint256) { return a * 10 + b; }
    function deepCallChain(uint256 v) external pure returns (uint256) { return _l1(v); }
    function _l1(uint256 v) internal pure returns (uint256) { return _l2(v + 1) * 2; }
    function _l2(uint256 v) internal pure returns (uint256) { return _l3(v + 1) * 3; }
    function _l3(uint256 v) internal pure returns (uint256) { return _l4(v + 1) * 5; }
    function _l4(uint256 v) internal pure returns (uint256) { return v + 1; }
    function pubStateGetter() external returns (uint256) { x = 42; return this_x(); }
    function this_x() internal view returns (uint256) { return x; }
    function viewInPure(uint256 v) external pure returns (uint256) { return _pureHelper(v) + _pureHelper(v); }
    function _pureHelper(uint256 v) private pure returns (uint256) { return v * v; }
    function structRet(uint256 v) external pure returns (P memory) { return _mkP(v); }
    struct P { uint256 a; uint256[2] b; }
    function _mkP(uint256 v) internal pure returns (P memory p) { p.a = v; p.b[1] = v; }
    function structArg(P memory p) external pure returns (uint256) { return _usep(p); }
    function _usep(P memory p) internal pure returns (uint256) { p.a += 1; return p.a + p.b[1]; }
    function structArgMut(uint256 v) external pure returns (uint256) { P memory p = _mkP(v); _usep(p); return p.a; }
}
function free(uint256 v) pure returns (uint256) { return v * 3; }
uint256 constant FREE_CONST = 11;
