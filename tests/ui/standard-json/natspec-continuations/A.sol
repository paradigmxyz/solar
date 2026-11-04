contract C {
    /// @dev Registers `operator` for the caller. This does NOT validate the
    ///  operator address, so the caller keeps full responsibility for it.
    /// @param operator The operator
    ///   address to register.
    /// @return ok Whether it
    ///   worked.
    function register(address operator) external returns (bool ok) {}

    /**
     * @notice Does a thing
     *   across lines.
     * @dev Dev text
     *   continued.
     */
    function block() external {}

    /// @notice Notice text
    ///   continued.
    function notice() external {}

    /// Untagged text
    ///   that continues.
    function untagged() external {}

    /// Intro text
    /// @dev Details after an untagged notice.
    function mixed() external {}

    /// @notice Notice text
    /// @param value Skips `@` blanks
    /// @ and drops it
    /// @return result   Keeps two blanks
    function last(uint256 value) external returns (uint256 result) {}
}
