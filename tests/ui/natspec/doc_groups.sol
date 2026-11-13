// solc reads only the last doc comment before a declaration: the last block comment, or the last
// run of `///` lines without a blank line or another comment between them.

/// @notice Superseded by the next group.

/// @dev Read.
uint constant BLANK_LINE = 1;

/// @notice Superseded by the block comment.
/** @dev Read. */
uint constant LINE_THEN_BLOCK = 1;

/** @notice Superseded by the line comment. */
/// @dev Read.
uint constant BLOCK_THEN_LINE = 1;

/// @notice Superseded too.
//// Four slashes start a plain comment.
/// @dev Read.
uint constant FOUR_SLASHES = 1;

/// @notice An empty doc line does not end the group.
///
/// @dev Read.
//~^^^ WARN: tag `@notice` not valid for file-level variables
uint constant EMPTY_DOC_LINE = 1;

contract C {
    /// @param x

    /// @param x The superseded tag is not checked.
    function f(uint x) public {}

    /// @custom:Old note
    /* A plain comment ends the group too. */
    /// @notice Read.
    function g() public {}
}
