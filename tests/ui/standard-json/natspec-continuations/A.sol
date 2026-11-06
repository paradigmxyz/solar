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

    /** @notice Block text
     ** glued
     *
     * and a blank star line
     */
    function stars() external {}

    /// @notice Notice.
    /// @dev Dev line one
    /// dev line two.
    function devContinuation() external {}

    /// @notice start
    /// explanation @dev mid-line details
    /// more
    function midLine() external {}

    /**
     * @notice x
     ** explanation @solidity memory-safe-assembly
     */
    function glued() external {}

    /**
     * Vault
     ** @dev glued on first
     */
    function gluedFirst() external {}

    /**
     * @notice x
     * y @ z
     ** w @solidity memory-safe-assembly
     */
    function emptyTag() external {}

    /**
     * @notice a
     * memory-safe-assembly
     ** @ z
     */
    function gluedEmptyTag() external {}

    /// @dev Dropped, a blank line ends the comment

    /// @notice Only the last comment counts
    function lastWins() external {}

    /// @dev Dropped, a plain comment ends the comment
    // plain
    /// @notice Kept after a plain comment
    function afterPlain() external {}

    /** @dev Dropped, a block comment before a line comment */
    /// @notice Kept after a block comment
    function afterBlock() external {}

    /// @notice Kept before a plain comment
    // plain
    function beforePlain() external {}

    /// @param a first
    /// @param a second
    function dupParam(uint256 a) external {}

    /// @return r
    function returnName() external returns (uint256 r) {}

    /// @return
    function bareReturn() external returns (uint256) {}

    /// @return amount The amount
    mapping(address owner => uint256 amount) public stateReturn;

    /// @return turn
    mapping(address owner => uint256 turn) public stateReturnName;
}
