pragma solidity ^0.8.20;
// A hash a quantified spec builds from its binders gets the facts a hash in the body gets:
// equal hashes of static encodings have equal words.
contract KeccakQuantified {
    /// @custom:ensures forall uint256 a :: forall uint256 b :: keccak256(abi.encode(a, uint256(1), address(0))) == keccak256(abi.encode(b, uint256(1), address(0))) ==> a == b
    function ok_injective() external pure {}

    /// @custom:ensures forall uint256 a :: forall address u :: forall address v :: keccak256(abi.encode(a, u)) == keccak256(abi.encode(a, v)) ==> u == v
    function ok_injective_address() external pure {}

    /// @custom:ensures forall uint256 a :: forall uint256 b :: keccak256(abi.encode(a, b)) == keccak256(abi.encode(b, a))
    function bad_symmetric() external pure {}

    // Fifteen words a side, as a settlement digest has.
    /// @custom:ensures forall uint256 a0 :: forall uint256 a1 :: forall uint256 a2 :: forall uint256 a3 :: forall uint256 a4 :: forall uint256 a5 :: forall uint256 a6 :: forall uint256 a7 :: forall uint256 a8 :: forall uint256 a9 :: forall uint256 a10 :: forall uint256 a11 :: forall uint256 a12 :: forall uint256 a13 :: forall uint256 a14 :: forall uint256 b0 :: forall uint256 b1 :: forall uint256 b2 :: forall uint256 b3 :: forall uint256 b4 :: forall uint256 b5 :: forall uint256 b6 :: forall uint256 b7 :: forall uint256 b8 :: forall uint256 b9 :: forall uint256 b10 :: forall uint256 b11 :: forall uint256 b12 :: forall uint256 b13 :: forall uint256 b14 :: keccak256(abi.encode(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14)) == keccak256(abi.encode(b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10, b11, b12, b13, b14)) ==> (a0 == b0 && a7 == b7 && a14 == b14)
    function ok_injective_wide() external pure {}
}
