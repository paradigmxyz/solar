// A named `@return` with no text documents its name, unless a later line continues it.

contract NameOnly {
    /**
     * @return a
     */
    function block() public returns (uint256 a) {}

    /// @return b
    ///   
    function blankLine() public returns (uint256 b) {}

    /// @return c
    /// @
    function loneAt() public returns (uint256 c) {}

    /// @return
    ///
    /// d after an empty line
    function emptyLine() public returns (uint256 d) {}
}
