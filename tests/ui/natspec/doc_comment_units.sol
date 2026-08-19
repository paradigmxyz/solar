// Only the last documentation unit binds to an item, like solc: a `/** */`
// block, or a contiguous run of `///` lines. Earlier units are silently
// dropped, no matter how many blank lines separate them from the item.

/// @notice File header describing the library.
/// @author Someone
/// @author Someone else

/// @dev Only this unit binds.
function detachedHeader() pure returns (uint256) {
    return 1;
}

/** @author Someone */
/// @dev A new unit supersedes the block comment.
function blockThenLine() pure returns (uint256) {
    return 1;
}

/// @author Someone
// An ordinary comment splits the run.
/// @dev Only this unit binds.
function splitByComment() pure returns (uint256) {
    return 1;
}

contract Contiguous {
    /// @author Someone
    /// @dev One contiguous unit, so the invalid tag binds and errors.
    //~^^ ERROR: tag `@author` not valid for functions
    function contiguous() public pure returns (uint256) {
        return 1;
    }
}
