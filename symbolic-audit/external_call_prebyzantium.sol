interface I { function f() external returns (uint256); function g() external; function h() external view returns (uint256); }
contract R {
    function callRet(address a) external returns (uint256) { return I(a).f(); }
    function callNoRet(address a) external { I(a).g(); }
    function callView(address a) external view returns (uint256) { return I(a).h(); }
    function callPtr(function() external returns (uint256) p) external returns (uint256) { return p(); }
    function callPtrNoRet(function() external p) external { p(); }
}
