contract Base {
    uint256 public log;
    modifier m1() virtual { log = log * 10 + 1; _; log = log * 10 + 2; }
    modifier m2(uint256 v) virtual { log = log * 10 + v; _; }
    modifier guard(bool c) { require(c, "guard"); _; }
    function f() public virtual m1 returns (uint256) { log = log * 10 + 5; return log; }
    function g(uint256 v) public virtual m2(v) m1 returns (uint256) { return log; }
    function h() internal virtual returns (uint256) { return 1; }
    function callH() external returns (uint256) { return h(); }
}
contract Mid is Base {
    modifier m1() virtual override { log = log * 10 + 3; _; log = log * 10 + 4; }
    function f() public virtual override returns (uint256) { return super.f() + 1000; }
    function h() internal virtual override returns (uint256) { return super.h() * 2; }
}
contract Leaf is Mid {
    modifier m2(uint256 v) override { log = log * 10 + v + 1; _; log = log * 10 + 9; }
    function f() public override m1 returns (uint256) { return super.f() + 100000; }
    function h() internal override returns (uint256) { return super.h() + 100; }
    function g2(uint256 v) external m2(v) returns (uint256) { return log; }
    function withGuard(bool c, uint256 v) external guard(c) m1 returns (uint256) { log += v; return log; }
    function modArgSide(uint256 v) external m2(_side(v)) m2(_side(v + 1)) returns (uint256) { return log; }
    function _side(uint256 v) internal returns (uint256) { log = log * 10 + 7; return v; }
    modifier retInMod(uint256 v) { if (v > 5) { log = 999; return; } _; }
    function withRet(uint256 v) external retInMod(v) returns (uint256 r) { r = v; log = v; }
    function withRetTwo(uint256 v) external retInMod(v) returns (uint256 r, bool b) { r = v; b = true; }
    modifier loopMod(uint256 n) { for (uint256 i; i < n; i++) { _; } }
    function withLoop(uint256 n) external loopMod(n) returns (uint256) { require(n < 5); log += 1; return log; }
    modifier condMod(bool c) { if (c) { _; } else { log = 42; } }
    function withCond(bool c) external condMod(c) returns (uint256 r) { log = 7; r = 1; }
    modifier nestedRevert() { _; require(log < 50, "late"); }
    function withLateRevert(uint256 v) external nestedRevert returns (uint256) { log = v; return v; }
    modifier localVar() { uint256 before = log; _; log = log + before; }
    function withLocal(uint256 v) external localVar returns (uint256) { log = v; return log; }
    modifier localVarTwice() { uint256 x = 1; _; x += 1; _; log += x; }
    function withLocalTwice(uint256 v) external localVarTwice returns (uint256) { log += v; return log; }
    modifier paramMod(uint256 v) { v += 1; log = v; _; }
    function withParamMod(uint256 v) external paramMod(v) returns (uint256) { return log + v; }
    modifier unchk(uint256 v) { unchecked { log = v + type(uint256).max; } _; }
    function withUnchk(uint256 v) external unchk(v) returns (uint256) { return log; }
    modifier virtualUse() { log = h(); _; }
    function withVirtual() external virtualUse returns (uint256) { return log; }
    function superChainF() external returns (uint256, uint256) { uint256 r = f(); return (r, log); }
    function baseF() external returns (uint256, uint256) { uint256 r = Base.f(); return (r, log); }
    function midF() external returns (uint256, uint256) { uint256 r = Mid.f(); return (r, log); }
    function superG(uint256 v) external returns (uint256) { return g(v); }
    function fnPtrVirtual(bool c) external returns (uint256) { function() internal returns (uint256) p = c ? Base.h : h; return p(); }
    function fnPtrSuper() external returns (uint256) { function() internal returns (uint256) p = Mid.h; return p(); }
}
