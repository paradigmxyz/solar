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

// solc rejects the invalid tag `@example.com` here, which ends the `@dev` text. We only warn about
// invalid tags that start a line, but this one still ends the text.
/**
 * @dev Mail
 *   admin@example.com for help.
 */
contract F {}
