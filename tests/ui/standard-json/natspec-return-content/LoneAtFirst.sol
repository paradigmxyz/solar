// A bare `@return` reads its name from the next line even when that line is a lone `@`, which
// solc rejects with error 5856.

contract LoneAtFirst {
    /// @return
    /// @
    /// f after a lone at sign
    function loneAtFirst() public returns (uint256 f) {}
}
