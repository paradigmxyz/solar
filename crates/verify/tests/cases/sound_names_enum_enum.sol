pragma solidity ^0.8.20;
// Two enums E with different variant counts and orders.
interface IA { enum E { P, Q, R } }
interface IB { enum E { Q } }
contract C {
    /// @custom:ensures r == 0
    function bad_enum_size(IA.E e) external pure returns (uint256 r) { r = uint256(e); }
    /// @custom:ensures r == 0
    function bad_enum_variant() external pure returns (uint256 r) { r = uint256(IA.E.Q); }
    /// @custom:ensures r == 0
    // Refused rather than resolved by bare name.
    // expect: share the name
    function bad_true_spec_enum(IB.E e) external pure returns (uint256 r) { r = uint256(e); }
}
