// `@custom:solar-inline [always|never]` asks the inliner to favor a function, to clone it at every
// legal call, or never to clone it. It documents an internal, private or public function with a
// body, takes at most one of those two arguments, and appears once.

contract C {
    /// @custom:solar-inline
    function hint(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    /// @custom:solar-inline always
    function always(uint256 x) internal pure returns (uint256) {
        return x + 2;
    }

    /// @custom:solar-inline never
    function never(uint256 x) private pure returns (uint256) {
        return x * 2;
    }

    /// @custom:solar-inline always
    //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
    function external_() external pure {}

    /// @custom:solar-inline never
    //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
    uint256 internal v;

    /// @custom:solar-inline sometimes
    //~^ ERROR: `@custom:solar-inline` takes no argument, `always` or `never`
    function unknown() internal pure {}

    /// @custom:solar-inline always never
    //~^ ERROR: `@custom:solar-inline` takes no argument, `always` or `never`
    function both() internal pure {}

    /// @custom:solar-inline always
    /// @custom:solar-inline never
    //~^ ERROR: duplicate `@custom:solar-inline` tag
    function twice() internal pure {}

    /// @custom:solar-noinline
    //~^ ERROR: unknown tag `@custom:solar-noinline`
    function removed() internal pure {}

    function use() external pure returns (uint256) {
        both();
        twice();
        unknown();
        removed();
        return hint(1) + always(1) + never(2);
    }
}
