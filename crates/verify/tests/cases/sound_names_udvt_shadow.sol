pragma solidity ^0.8.20;
// A contract-level UDVT U shadows a later file-level UDVT U with a narrower underlying type.
contract C {
    type U is uint256;
    /// @custom:ensures r < 256
    function bad_udvt_shadow(U x) external pure returns (uint256 r) { r = U.unwrap(x); }
    /// @custom:ensures r == 300
    // Refused rather than resolved by bare name.
    // expect: share the name
    function bad_true_spec_udvt_shadow() external pure returns (uint256 r) { r = U.unwrap(U.wrap(300)); }
}
type U is uint8;
