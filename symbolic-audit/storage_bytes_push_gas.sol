contract P {
    bytes a; uint8[] u;
    function pushArg(uint256 n) external { for (uint256 i; i < n; i++) a.push(bytes1(uint8(i))); }
    function pushNoArg(uint256 n) external { for (uint256 i; i < n; i++) a.push(); }
    function pushNoArgAssign(uint256 n) external { for (uint256 i; i < n; i++) a.push() = bytes1(uint8(i)); }
    function popAll() external { uint256 n = a.length; for (uint256 i; i < n; i++) a.pop(); }
    function pushU8(uint256 n) external { for (uint256 i; i < n; i++) u.push(uint8(i)); }
    function len() external view returns (uint256) { return a.length; }
}
