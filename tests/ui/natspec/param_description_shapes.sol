// The description of a `@param` tag must start on the tag's own line.

contract C {
    /// @param x
    //~^ WARN: tag `@param` does not describe parameter `x`
    ///        Continuation lines do not count.
    function nextLine(uint x) public {}

    /**
     * @param x
     */
    function block(uint x) public {}
    //~^^^ WARN: tag `@param` does not describe parameter `x`

    /** @param x */
    //~^ WARN: tag `@param` does not describe parameter `x`
    function inlineBlock(uint x) public {}

    /// @param x	Tab-separated descriptions are fine.
    /// @param -   Unnamed parameters are documented with `-`.
    function described(uint x, uint) public {}
}

/// @param a Structs are not validated, so the next tag needs no description.
/// @param b
struct Undescribed {
    uint a;
}
