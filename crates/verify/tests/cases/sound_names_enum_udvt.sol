pragma solidity ^0.8.20;
// A UDVT named like an enum elsewhere.
interface I { enum T { X, Y } }
type T is uint256;
contract C {
    /// @custom:ensures r < 2
    function bad_enum_udvt(T x) external pure returns (uint256 r) { r = T.unwrap(x); }
    /// @custom:ensures r < 2
    // Refused rather than resolved by bare name.
    // expect: share the name
    function bad_true_spec_enum_udvt(I.T x) external pure returns (uint256 r) { r = uint256(x); }
}
