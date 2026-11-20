contract FunctionRegions {
    function first(uint256 x) external pure returns (uint256) { return helper(x); }
    function second(uint256 x) external pure returns (uint256) { return x ^ 0; }

    /// @custom:solar-inline never
    function helper(uint256 x) internal pure returns (uint256) {
        unchecked { uint256 dead = x * x; return x + 1; }
    }
}
