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
        {
            x += abi.encode(x).length;
        }
        /// @custom:solar-scratch
        unchecked {
            x += abi.encode(x).length;
        }
        /// @custom:solar-scratch
        //~^ ERROR: `@custom:solar-scratch` must document a block statement
        x += 1;
        /// @custom:solar-scratch
        //~^ ERROR: `@custom:solar-scratch` must document a block statement
        for (uint256 i; i < 2; ++i) {
            x += abi.encode(i).length;
        }
        /// @custom:solar-scratch
        {
            assembly {
                //~^ ERROR: a `@custom:solar-scratch` block cannot contain inline assembly
                x := add(x, 1)
            }
        }
        /// @custom:unrelated
        uint256 y = 2;
        x += y;
    }

    /// @custom:solar-scratch
    //~^ ERROR: `@custom:solar-scratch` must document a block statement
    function i() internal pure {}

    /// @custom:solar-trusted
    function g() internal pure {}

    /// @custom:solar-safe
    //~^ ERROR: `@custom:solar-safe` must document a contract or a library
    function h() external pure {}

    /// @custom:solar-trusted
    //~^ ERROR: `@custom:solar-trusted` must document a function, a modifier, a contract, or a library
    event E();
}
