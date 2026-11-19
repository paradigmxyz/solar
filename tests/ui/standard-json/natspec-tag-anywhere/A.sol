// solc starts a NatSpec tag at the first `@` of a line, wherever it is, and drops the text
// before it on that line.

contract C {
    /// @dev after a non-breaking space
    function f() public {}

    /// see foo @dev mid line
    function g() public {}

    /// @dev first line
    /// second line
    function h() public {}

    /// @notice mail user@example.com
    function i() public {}
}

/**
 * @dev Example:
 * /// @custom:x y
 *   continued
 */
contract D {}
