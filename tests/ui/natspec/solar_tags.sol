// `@custom:solar-*` tags state requirements this compiler checks, which other
// compilers read as documentation. A tag in the wrong place or with an unknown
// name is an error rather than a requirement that silently goes unchecked;
// other `@custom:` tags stay documentation.

/// @custom:solar-saef
//~^ ERROR: unknown tag `@custom:solar-saef`
/// @custom:unrelated
contract Test {
    function f(bytes memory b) public pure returns (uint256 x) {
        /// @custom:solar-inline
        //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
        x = b.length;
        /// @custom:solar-optimize size
        //~^ ERROR: `@custom:solar-optimize` must document a non-abstract contract or a library
        x += 1;
        /// @custom:solar-safe
        //~^ ERROR: `@custom:solar-safe` must document a contract or a library
        for (uint256 i; i < 2; ++i) {
            x += abi.encode(i).length;
        }
        /// @custom:unrelated
        uint256 y = 2;
        x += y;
        /// @custom:solar-trusted
        //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
        {
            x += 1;
        }
        /// @custom:solar-view
        //~^ ERROR: unknown tag `@custom:solar-view`
        (x, ) = (1, 2);
    }

    /// @custom:solar-scratch
    //~^ ERROR: unknown tag `@custom:solar-scratch`
    function i() internal pure {}

    /// @custom:solar-trusted
    function g() internal pure {}

    /// @custom:solar-safe
    //~^ ERROR: `@custom:solar-safe` must document a contract or a library
    function h() external pure {}

    /// @custom:solar-trusted
    //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
    event E();

    /// @custom:solar-terminates
    //~^ ERROR: unknown tag `@custom:solar-terminates`
    function stop() internal pure { revert(); }
}
