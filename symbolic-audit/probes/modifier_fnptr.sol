contract ModifierFnptr {
    uint256 log_;
    function tick(uint256 t) internal returns (uint256) { log_ = log_ * 10 + t; return t; }
    modifier ma(uint256 v) { tick(v); _; }
    modifier mb(uint256 v) { tick(v); _; tick(v + 1); }
    function two() external ma(tick(1)) mb(tick(2)) returns (uint256) { tick(5); return log_; }
    function twoRev() external mb(tick(2)) ma(tick(1)) returns (uint256) { tick(5); return log_; }
    function argsAndMods(uint256 x) external ma(tick(x)) returns (uint256) { return log_; }
    function callWithMods() external returns (uint256) { return this.two(); }
    function modReturns() external ma(tick(1)) returns (uint256 r) { r = 7; }
    modifier twice() { _; _; }
    function runTwice() external twice returns (uint256) { tick(3); return log_; }
    modifier skip() { if (log_ > 100) { _; } }
    function skipped() external skip returns (uint256) { tick(1); return 5; }

    function(uint256) internal returns (uint256) fp;
    function(uint256) internal returns (uint256)[2] fps;
    mapping(uint256 => function(uint256) internal returns (uint256)) fpm;
    function callUninit(uint256 x) external returns (uint256) { return fp(x); }
    function callArrUninit(uint256 x) external returns (uint256) { return fps[1](x); }
    function callMapUninit(uint256 x) external returns (uint256) { return fpm[3](x); }
    function callSet(uint256 x) external returns (uint256) { fp = tick; return fp(x); }
    function callArrSet(uint256 x) external returns (uint256) { fps[0] = tick; fps[1] = double; return fps[1](x) + fps[0](x); }
    function double(uint256 x) internal pure returns (uint256) { return 2 * x; }
    function localPtr(uint256 x) external returns (uint256) { function(uint256) internal returns (uint256) f = x > 5 ? tick : double; return f(x); }
    function ptrEq() external returns (bool) { function(uint256) internal returns (uint256) f = tick; return f == tick; }
    function externalPtrCall(uint256 x) external returns (uint256) { function(uint256) external returns (uint256) f = this.argsAndMods; return f(x); }
    function externalPtrBadAddr(uint256 x) external returns (uint256) { function(uint256) external returns (uint256) f = this.argsAndMods; assembly { f.address := 0xdead } return f(x); }
    function externalPtrBadSel(uint256 x) external returns (uint256) { function(uint256) external returns (uint256) f = this.argsAndMods; assembly { f.selector := 0x12345678 } return f(x); }
    function selectorOf() external view returns (bytes4) { return this.argsAndMods.selector; }
    function deleteFp(uint256 x) external returns (uint256) { fp = tick; delete fp; return fp(x); }
}
