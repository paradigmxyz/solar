// A named return's `@return` text drops the name and one space or tab after it, and keeps the
// rest as is, like Seaport's `ConsiderationInterface.information()` with its aligned columns.

interface Seaport {
    /// @return version          The contract version.
    /// @return domainSeparator	The domain separator.
    /// @return conduitController
    function information()
        external
        view
        returns (string memory version, bytes32 domainSeparator, address conduitController);

    /**
     * @return
     *   status   on the next line
     */
    function status() external view returns (uint256 status);
}

contract Getter {
    struct Pair {
        uint256 x;
        uint256 y;
    }

    /// @return x  the first
    Pair public pair;
}
