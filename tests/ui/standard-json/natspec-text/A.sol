// The documentation outputs join and trim NatSpec text like solc.

/// line one
///   line two
contract C {
    /**
     * @notice block notice
     *   continued
     * @dev dev text
     */
    function f() public pure {}

    /// @return a first
    ///   more
    /// @return b second
    function g() public pure returns (uint256 a, uint256 b) {}

    /**
     * Plain block
     */
    function h() public pure {}

    /** single */
    function i() public pure {}

    /// @param x the value
    /// that continues
    function j(uint256 x) public pure {}
}

interface I {
    /// @notice inherited notice
    /// @dev inherited dev
    /// @custom:tag not inherited
    function k(uint256 v) external;

    /// @notice renamed parameter
    function m(uint256 v) external;
}

contract E is I {
    function k(uint256 v) external {}

    function m(uint256 w) external {}
}
