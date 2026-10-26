// The range of an unreachable modifier includes its NatSpec, and a placeholder ends at `_`.
contract C {
    uint x;

    /// @notice one
    /// @dev two
    modifier r() { revert(); _; }

    //~v WARN: unreachable code
    /**
     * @notice block
     */
    modifier m() { x = 1; _; }

    //~v WARN: unreachable code
    /// @notice after
    modifier n() { x = 2; _; }

    modifier ifPlaceholder(bool c) { revert(); if (c) _; } //~ WARN: unreachable code

    modifier whilePlaceholder(bool c) { revert(); while (c) _; } //~ WARN: unreachable code

    function f() public r m n {} //~ WARN: unreachable code

    function g(bool c) public ifPlaceholder(c) {} //~ WARN: unreachable code

    function h(bool c) public whilePlaceholder(c) {} //~ WARN: unreachable code
}
