pragma solidity ^0.8.20;
/// Spec errors are reported before any solver runs.
contract SpecErrors {
    uint256 x;

    /// @custom:ensures forall uint256 i i > 0
    function noSeparator() external {}

    /// @custom:ensures x == 1 x
    function trailing() external { x = 1; }

    /// @custom:check x == old(x)
    function checkOld() external {}

    function orphan() external {
        x = 1;
        /// @custom:ghost x = 2
    }
}
