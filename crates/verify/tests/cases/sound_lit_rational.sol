pragma solidity ^0.8.20;
// Literal division is rational in Solidity (7 / 2 == 3.5, so (7 / 2) * 2 == 7), but the
// verifier folds it as integer floor division.
contract RatLitDiv {
    /// @custom:ensures result == 6
    function bad_div() public pure returns (uint256) { uint256 r = (7 / 2) * 2; return r; }
    /// @custom:ensures result == 7
    function ok_div() public pure returns (uint256) { uint256 r = (7 / 2) * 2; return r; }
    /// @custom:ensures result == -8
    function bad_sdiv() public pure returns (int256) { int256 r = (-7 / 2) * 2; return r; }
    /// @custom:ensures result == 10**18
    function bad_wad() public pure returns (uint256) { return (10**18 / 3) * 3 + 1; }
}
