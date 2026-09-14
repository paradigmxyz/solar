library L {
    function f(uint256 x) external pure returns (uint256) { return x + 1; }
}

contract C {
    function f(uint256 x) external pure returns (uint256) { return L.f(x); }
}
