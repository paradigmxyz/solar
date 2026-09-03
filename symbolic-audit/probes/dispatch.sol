contract DispatchFallback {
    uint256 public last;
    fallback(bytes calldata d) external returns (bytes memory) { last = d.length; return abi.encodePacked(uint8(1), d); }
    function f(uint256 x) external pure returns (uint256) { return x + 1; }
    function g() external pure returns (uint256) { return 2; }
}
contract DispatchNoFallback {
    function f(uint256 x) external pure returns (uint256) { return x + 1; }
    function g() external pure returns (uint256) { return 2; }
}
contract DispatchReceiveOnly {
    receive() external payable {}
    function f(uint256 x) external pure returns (uint256) { return x + 1; }
}
contract DispatchPlainFallback {
    uint256 public hits;
    fallback() external { hits++; }
    function f(uint256 x) external pure returns (uint256) { return x + 1; }
}
