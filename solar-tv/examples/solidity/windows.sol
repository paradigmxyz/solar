contract WindowRegions {
    function combine(uint256 a, uint256 b) external payable returns (uint256) {
        unchecked { if (a == b) return a + 0; return a ^ b; }
    }
    function another(uint256 a, uint256 b, bool c) external payable returns (uint256) {
        unchecked { if (c) return (a + b) * (a ^ b); return (a - b) ^ b; }
    }
}
