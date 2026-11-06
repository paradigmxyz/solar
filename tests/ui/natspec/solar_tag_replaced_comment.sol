// Solc keeps only the last doc comment before an item, so a `@custom:solar-*` tag in an earlier
// one would silently go unchecked. It is an error instead.

//~v ERROR: this doc comment is replaced by a later one
/// @custom:solar-safe memory

/// @notice The comment that documents the contract.
contract Replaced {
    function f() external pure returns (uint256) {
        return 1;
    }
}

//~v ERROR: this doc comment is replaced by a later one
/// @custom:solar-optimize size
// A plain comment does not replace the doc comment.
/// @notice The comment that documents the contract.
contract ReplacedByPlainComment {}

/// @custom:solar-optimize size
// A plain comment after the last doc comment does not replace it.
contract Kept {}
