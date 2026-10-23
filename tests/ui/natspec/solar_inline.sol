contract C {
    /// @custom:solar-inline
    function f(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    /// @custom:solar-noinline
    function g(uint256 x) private pure returns (uint256) {
        return x * 2;
    }

    /// @custom:solar-inline
    //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
    function h() external pure {}

    /// @custom:solar-noinline
    //~^ ERROR: `@custom:solar-noinline` must document a function that internal calls reach
    uint256 internal v;

    /// @custom:solar-inline
    //~^ ERROR: a function cannot be both `@custom:solar-inline` and `@custom:solar-noinline`
    /// @custom:solar-noinline
    function both() internal pure {}

    function use() external pure returns (uint256) {
        both();
        return f(1) + g(2);
    }
}
