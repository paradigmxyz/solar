pragma solidity ^0.8.20;
// A contract named like an enum elsewhere.
interface I { enum W { X, Y } }
contract W {}
contract C {
    /// @custom:ensures r < 2
    function bad_enum_contract(W w) external pure returns (uint256 r) { r = uint256(uint160(address(w))); }
    /// @custom:ensures r < 2
    // Refused rather than resolved by bare name.
    // expect: share the name
    function bad_true_spec_enum_contract(I.W w) external pure returns (uint256 r) { r = uint256(w); }
}
