// `@custom:solar-*` tags state requirements this compiler checks, which other
// compilers read as documentation. A tag in the wrong place or with an unknown
// name is an error rather than a requirement that silently goes unchecked;
// other `@custom:` tags stay documentation.

/// @custom:solar-saef
//~^ ERROR: unknown tag `@custom:solar-saef`
/// @custom:unrelated
contract Test {
    /// @custom:solar-view
    //~^ ERROR: unknown tag `@custom:solar-view`
    function f(bytes memory b) public pure returns (uint256 x) {
        /// @custom:solar-inline
        //~^ ERROR: `@custom:solar-inline` must document a function that internal calls reach
        x = b.length;
        /// @custom:solar-scratch
        //~^ ERROR: unknown tag `@custom:solar-scratch`
        {
            x += abi.encode(x).length;
        }
        /// @custom:unrelated
        uint256 y = 2;
        x += y;
    }

    /// @custom:solar-trusted
    function g() internal pure {}

    /// @custom:solar-safe
    //~^ ERROR: `@custom:solar-safe` must document a contract or a library
    function h() external pure {}

    /// @custom:solar-trusted
    //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
    event E();
}
