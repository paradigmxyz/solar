// A named `@return` with no text documents its name, unless a later line continues it.

contract NameOnly {
    /**
     * @return a
     */
    function block() public returns (uint256 a) {}

    /// @return b
    ///   
    function blankLine() public returns (uint256 b) {}
}
