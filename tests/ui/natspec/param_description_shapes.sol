// The description of a `@param` tag must start on the tag's own line.

contract C {
    /// @param x
    ///        Continuation lines do not count.
    //~^^ WARN: tag `@param` does not describe parameter `x`
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

    /// @param x 
    function nonBreakingSpace(uint x) public {}

    /// @param
    ///
    /// x A tag with nothing after it reads the name from the next line.
    function nameOnNextLine(uint x) public {}

    /**
     * @param
     * x The same holds in block comments.
     */
    function nameOnNextBlockLine(uint x) public {}

    /// @param 5 Names start with a letter, so this documents an empty name.
    //~^ WARN: tag `@param` references non-existent parameter ''
    function digit(uint x) public {}
}

/// @param
//~^ WARN: tag `@param` does not contain a parameter name
contract Malformed {
    /// @param x
    //~^ WARN: tag `@param` does not describe parameter `x`
    uint public v;
}

/// @param a Structs are not validated, so the next tag needs no description.
/// @param b
struct Undescribed {
    uint a;
}
